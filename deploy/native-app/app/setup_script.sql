CREATE APPLICATION ROLE IF NOT EXISTS app_user;

CREATE SCHEMA IF NOT EXISTS core;
GRANT USAGE ON SCHEMA core TO APPLICATION ROLE app_user;

CREATE OR ALTER VERSIONED SCHEMA app_public;
GRANT USAGE ON SCHEMA app_public TO APPLICATION ROLE app_user;

CREATE TABLE IF NOT EXISTS core.rustice_config (
  key STRING,
  value STRING
);

CREATE SECRET IF NOT EXISTS core.rustice_jwt_secret
  TYPE = GENERIC_STRING
  SECRET_STRING = 'rustice-native-app-spcs-trusted-ingress';

CREATE OR REPLACE PROCEDURE app_public.configure_external_access(
  horizon_database STRING,
  horizon_role STRING,
  client_database STRING,
  client_schema STRING,
  horizon_schemas STRING,
  horizon_tables STRING,
  s3_region STRING,
  extra_egress_hosts STRING
)
RETURNS STRING
LANGUAGE SQL
EXECUTE AS OWNER
AS
$$
DECLARE
  account_identifier STRING;
  account_locator STRING;
  current_region STRING;
  cloud_name STRING;
  region_name STRING;
  effective_s3_region STRING;
  catalog_host STRING;
  catalog_url STRING;
  snowflake_issuer_host STRING;
  s3_host STRING;
  all_hosts STRING;
  host_values STRING DEFAULT '';
  eai_name STRING;
  network_rule_fqn STRING;
  create_network_rule_sql STRING;
  create_eai_sql STRING;
  app_spec_sql STRING;
BEGIN
  account_identifier := LOWER(REPLACE(CURRENT_ORGANIZATION_NAME() || '-' || CURRENT_ACCOUNT_NAME(), '_', '-'));
  account_locator := LOWER(CURRENT_ACCOUNT());
  current_region := CURRENT_REGION();
  cloud_name := LOWER(SPLIT_PART(current_region, '_', 1));
  region_name := LOWER(REPLACE(REGEXP_REPLACE(current_region, '^[^_]+_', ''), '_', '-'));
  effective_s3_region := COALESCE(NULLIF(s3_region, ''), IFF(STARTSWITH(current_region, 'AWS_'), LOWER(REPLACE(REGEXP_REPLACE(current_region, '^AWS_', ''), '_', '-')), ''));

  catalog_host := account_identifier || '.snowflakecomputing.com';
  catalog_url := 'https://' || catalog_host || '/polaris/api/catalog';
  snowflake_issuer_host := account_locator || '.' || region_name || '.' || cloud_name || '.snowflakecomputing.com';
  s3_host := IFF(effective_s3_region = '', '', 's3.' || effective_s3_region || '.amazonaws.com');
  all_hosts := catalog_host || IFF(s3_host = '', '', ',' || s3_host) || IFF(COALESCE(extra_egress_hosts, '') = '', '', ',' || extra_egress_hosts);

  SELECT COALESCE(LISTAGG(host_literal, ', '), '')
  INTO :host_values
  FROM (
    SELECT DISTINCT
      '''' || REPLACE(TRIM(value::STRING), '''', '''''') || '''' AS host_literal
    FROM TABLE(SPLIT_TO_TABLE(:all_hosts, ','))
    WHERE TRIM(value::STRING) <> ''
  );

  IF (host_values = '') THEN
    RETURN 'No external access hosts resolved';
  END IF;

  eai_name := CURRENT_DATABASE() || '_RUSTICE_EAI';
  network_rule_fqn := CURRENT_DATABASE() || '.CORE.RUSTICE_EGRESS_RULE';

  create_network_rule_sql := 'CREATE OR REPLACE NETWORK RULE ' || network_rule_fqn ||
    ' TYPE = HOST_PORT MODE = EGRESS VALUE_LIST = (' || host_values || ')';
  EXECUTE IMMEDIATE create_network_rule_sql;

  create_eai_sql := 'CREATE OR REPLACE EXTERNAL ACCESS INTEGRATION ' || eai_name ||
    ' ALLOWED_NETWORK_RULES = (' || network_rule_fqn || ')' ||
    ' ENABLED = TRUE';
  EXECUTE IMMEDIATE create_eai_sql;

  app_spec_sql := 'ALTER APPLICATION SET SPECIFICATION rustice_external_access' ||
    ' TYPE = EXTERNAL_ACCESS' ||
    ' LABEL = ''Rustice Horizon and object-store egress''' ||
    ' DESCRIPTION = ''Allows Rustice to reach Snowflake Horizon Catalog and object storage endpoints.''' ||
    ' HOST_PORTS = (' || host_values || ')';
  EXECUTE IMMEDIATE app_spec_sql;

  DELETE FROM core.rustice_config;
  INSERT INTO core.rustice_config(key, value)
  SELECT * FROM VALUES
    ('horizon_database', :horizon_database),
    ('horizon_role', :horizon_role),
    ('client_database', :client_database),
    ('client_schema', :client_schema),
    ('horizon_schemas', :horizon_schemas),
    ('horizon_tables', :horizon_tables),
    ('s3_region', :effective_s3_region),
    ('catalog_url', :catalog_url),
    ('snowflake_issuer_host', :snowflake_issuer_host),
    ('eai_name', :eai_name),
    ('image_path', '/RUSTICE_NATIVE_APP_IMAGES/PUBLIC/RUSTICE_REPO/rustice:latest');

  RETURN 'Configured Rustice external access hosts: ' || all_hosts ||
    '. Approve the app specification if Snowsight shows it as pending, then call APP_PUBLIC.START_APP().';
END;
$$;

CREATE OR REPLACE PROCEDURE app_public.start_app()
RETURNS STRING
LANGUAGE SQL
EXECUTE AS OWNER
AS
$$
DECLARE
  pool_name STRING;
  eai_name STRING;
  image_path STRING;
  horizon_database STRING;
  horizon_role STRING;
  client_database STRING;
  client_schema STRING;
  horizon_schemas STRING;
  horizon_tables STRING;
  s3_region STRING;
  catalog_url STRING;
  snowflake_issuer_host STRING;
  create_pool_sql STRING;
  create_service_sql STRING;
BEGIN
  SELECT
    MAX(IFF(key = 'eai_name', value, NULL)),
    MAX(IFF(key = 'image_path', value, NULL)),
    MAX(IFF(key = 'horizon_database', value, NULL)),
    MAX(IFF(key = 'horizon_role', value, NULL)),
    MAX(IFF(key = 'client_database', value, NULL)),
    MAX(IFF(key = 'client_schema', value, NULL)),
    MAX(IFF(key = 'horizon_schemas', value, NULL)),
    MAX(IFF(key = 'horizon_tables', value, NULL)),
    MAX(IFF(key = 's3_region', value, NULL)),
    MAX(IFF(key = 'catalog_url', value, NULL)),
    MAX(IFF(key = 'snowflake_issuer_host', value, NULL))
  INTO
    :eai_name,
    :image_path,
    :horizon_database,
    :horizon_role,
    :client_database,
    :client_schema,
    :horizon_schemas,
    :horizon_tables,
    :s3_region,
    :catalog_url,
    :snowflake_issuer_host
  FROM core.rustice_config;

  IF (eai_name IS NULL) THEN
    RETURN 'Rustice is not configured. Call APP_PUBLIC.CONFIGURE_EXTERNAL_ACCESS(...) first.';
  END IF;

  pool_name := CURRENT_DATABASE() || '_RUSTICE_POOL';
  create_pool_sql := 'CREATE COMPUTE POOL IF NOT EXISTS ' || pool_name ||
    ' MIN_NODES = 1 MAX_NODES = 1 INSTANCE_FAMILY = CPU_X64_XS AUTO_RESUME = TRUE';
  EXECUTE IMMEDIATE create_pool_sql;

  DROP SERVICE IF EXISTS core.rustice_service;

  create_service_sql := 'CREATE SERVICE core.rustice_service' ||
    ' IN COMPUTE POOL ' || pool_name ||
    ' FROM SPECIFICATION_TEMPLATE_FILE = ''/service/rustice_spec.yaml''' ||
    ' USING (' ||
    ' image => ''' || REPLACE(image_path, '''', '''''') || ''',' ||
    ' rust_log => ''info'',' ||
    ' snowflake_issuer_host => ''' || REPLACE(snowflake_issuer_host, '''', '''''') || ''',' ||
    ' catalog_url => ''' || REPLACE(catalog_url, '''', '''''') || ''',' ||
    ' horizon_database => ''' || REPLACE(horizon_database, '''', '''''') || ''',' ||
    ' horizon_role => ''' || REPLACE(horizon_role, '''', '''''') || ''',' ||
    ' client_database => ''' || REPLACE(client_database, '''', '''''') || ''',' ||
    ' client_schema => ''' || REPLACE(client_schema, '''', '''''') || ''',' ||
    ' horizon_schemas => ''' || REPLACE(horizon_schemas, '''', '''''') || ''',' ||
    ' horizon_tables => ''' || REPLACE(COALESCE(horizon_tables, ''), '''', '''''') || ''',' ||
    ' horizon_eager_load => ''0'',' ||
    ' s3_region => ''' || REPLACE(COALESCE(s3_region, ''), '''', '''''') || ''',' ||
    ' jwt_secret => ''' || CURRENT_DATABASE() || '.CORE.RUSTICE_JWT_SECRET''' ||
    ' )' ||
    ' AUTO_SUSPEND_SECS = 0' ||
    ' EXTERNAL_ACCESS_INTEGRATIONS = (' || eai_name || ')' ||
    ' AUTO_RESUME = TRUE MIN_INSTANCES = 1 MAX_INSTANCES = 1';

  EXECUTE IMMEDIATE create_service_sql;

  RETURN 'Rustice service created. Call APP_PUBLIC.SERVICE_STATUS() and APP_PUBLIC.SERVICE_ENDPOINTS().';
END;
$$;

CREATE OR REPLACE PROCEDURE app_public.service_status()
RETURNS TABLE ()
LANGUAGE SQL
EXECUTE AS OWNER
AS
$$
BEGIN
  LET stmt STRING := 'SHOW SERVICE CONTAINERS IN SERVICE core.rustice_service';
  LET res RESULTSET := (EXECUTE IMMEDIATE :stmt);
  RETURN TABLE(res);
END;
$$;

CREATE OR REPLACE PROCEDURE app_public.service_endpoints()
RETURNS TABLE ()
LANGUAGE SQL
EXECUTE AS OWNER
AS
$$
BEGIN
  LET stmt STRING := 'SHOW ENDPOINTS IN SERVICE core.rustice_service';
  LET res RESULTSET := (EXECUTE IMMEDIATE :stmt);
  RETURN TABLE(res);
END;
$$;

CREATE OR REPLACE PROCEDURE app_public.suspend_app()
RETURNS STRING
LANGUAGE SQL
EXECUTE AS OWNER
AS
$$
DECLARE
  pool_name STRING;
BEGIN
  ALTER SERVICE IF EXISTS core.rustice_service SUSPEND;
  pool_name := CURRENT_DATABASE() || '_RUSTICE_POOL';
  EXECUTE IMMEDIATE 'ALTER COMPUTE POOL IF EXISTS ' || pool_name || ' SUSPEND';
  RETURN 'Rustice service and compute pool suspended';
END;
$$;

CREATE OR REPLACE PROCEDURE app_public.resume_app()
RETURNS STRING
LANGUAGE SQL
EXECUTE AS OWNER
AS
$$
DECLARE
  pool_name STRING;
BEGIN
  pool_name := CURRENT_DATABASE() || '_RUSTICE_POOL';
  EXECUTE IMMEDIATE 'ALTER COMPUTE POOL IF EXISTS ' || pool_name || ' RESUME';
  ALTER SERVICE IF EXISTS core.rustice_service RESUME;
  RETURN 'Rustice service and compute pool resume requested';
END;
$$;

GRANT USAGE ON PROCEDURE app_public.configure_external_access(STRING, STRING, STRING, STRING, STRING, STRING, STRING, STRING)
  TO APPLICATION ROLE app_user;
GRANT USAGE ON PROCEDURE app_public.start_app()
  TO APPLICATION ROLE app_user;
GRANT USAGE ON PROCEDURE app_public.service_status()
  TO APPLICATION ROLE app_user;
GRANT USAGE ON PROCEDURE app_public.service_endpoints()
  TO APPLICATION ROLE app_user;
GRANT USAGE ON PROCEDURE app_public.suspend_app()
  TO APPLICATION ROLE app_user;
GRANT USAGE ON PROCEDURE app_public.resume_app()
  TO APPLICATION ROLE app_user;
