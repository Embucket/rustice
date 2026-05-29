-- SQL-only Snowpark Container Services deployment for Rustice/Embucket.
--
-- This file mirrors the default Snowflake-side infrastructure created by
-- deploy/spcs/deploy.sh:
--   - database/schema/image repository
--   - CPU_X64_XS compute pool
--   - external access integration for Horizon and object-store reads
--   - service users and role-restricted PATs
--   - JWT and Horizon PAT secrets
--   - public SPCS service with executeAsCaller
--   - ingress-only service user/PAT for embucket-snow
--
-- Important limitation:
-- SQL cannot pull/build/tag/push Docker images. Before CREATE SERVICE can
-- succeed, the image below must already exist in the Snowflake image
-- repository. The deploy.sh script can do that automatically; with this SQL
-- file, create the image repository first, then push the image manually if it
-- is not already present.
--
-- Manual image push example after the image repository exists:
--   snow spcs image-registry login -c <connection>
--   docker pull --platform linux/amd64 embucket/rustice:latest
--   docker tag embucket/rustice:latest \
--     <org>-<account>.registry.snowflakecomputing.com/rustice_app/public/rustice_repo/rustice:latest
--   docker push \
--     <org>-<account>.registry.snowflakecomputing.com/rustice_app/public/rustice_repo/rustice:latest

-- ---------------------------------------------------------------------------
-- 0. Parameters
-- ---------------------------------------------------------------------------
-- Change RUSTICE_HORIZON_DATABASE and RUSTICE_HORIZON_ROLE before running in a
-- real account. The Horizon role must be able to access the target
-- Snowflake-managed Iceberg database/schema/tables.

SET RUSTICE_HORIZON_DATABASE = 'RUSTICE_SPCS';
SET RUSTICE_HORIZON_ROLE = 'DATA_ENGINEER';
SET RUSTICE_CLIENT_DATABASE = 'rustice_spcs';
SET RUSTICE_CLIENT_SCHEMA = 'public';

-- Object names match deploy.sh defaults.
SET RUSTICE_DB = 'RUSTICE_APP';
SET RUSTICE_SCHEMA = 'PUBLIC';
SET RUSTICE_COMPUTE_POOL = 'RUSTICE_POOL';
SET RUSTICE_IMAGE_REPOSITORY = 'RUSTICE_REPO';
SET RUSTICE_SERVICE = 'RUSTICE_SERVICE';
SET RUSTICE_CONTAINER_NAME = 'rustice';
SET RUSTICE_ENDPOINT_NAME = 'main';
SET RUSTICE_SERVICE_ROLE = 'rustice_user';
SET RUSTICE_INSTANCE_FAMILY = 'CPU_X64_XS';
SET RUSTICE_POOL_MIN_NODES = 1;
SET RUSTICE_POOL_MAX_NODES = 1;
SET RUSTICE_IMAGE_TAG = 'latest';

-- Catalog bootstrap defaults mirror deploy.sh. Set RUSTICE_HORIZON_TABLES to a
-- comma-separated list such as PUBLIC.SMOKE when you want tables registered at
-- startup without eager namespace listing.
SET RUSTICE_HORIZON_SCHEMAS = 'PUBLIC,public';
SET RUSTICE_HORIZON_TABLES = '';
SET RUSTICE_HORIZON_EAGER_LOAD = '0';
SET RUSTICE_HORIZON_EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';
SET RUSTICE_HORIZON_CATALOG = 'SNOWFLAKE';
SET RUSTICE_S3_REGION = '';

-- Service users/PATs mirror deploy.sh defaults.
SET RUSTICE_HORIZON_SERVICE_USER = 'RUSTICE_HORIZON_SVC';
SET RUSTICE_HORIZON_PAT_NAME = 'RUSTICE_HORIZON_PAT';
SET RUSTICE_HORIZON_PAT_DAYS = 15;
SET RUSTICE_INGRESS_ROLE = 'RUSTICE_INGRESS_ROLE';
SET RUSTICE_INGRESS_SERVICE_USER = 'RUSTICE_INGRESS_SVC';
SET RUSTICE_INGRESS_PAT_NAME = 'RUSTICE_INGRESS_PAT';
SET RUSTICE_INGRESS_PAT_DAYS = 1;

-- ---------------------------------------------------------------------------
-- 1. Derived values
-- ---------------------------------------------------------------------------
-- These values are resolved from the active Snowflake account, matching the
-- deploy.sh default behavior.

SET RUSTICE_ACCOUNT_IDENTIFIER = (
  SELECT LOWER(REPLACE(CURRENT_ORGANIZATION_NAME() || '-' || CURRENT_ACCOUNT_NAME(), '_', '-'))
);
SET RUSTICE_ACCOUNT_LOCATOR = (SELECT LOWER(CURRENT_ACCOUNT()));
SET RUSTICE_CURRENT_REGION = (SELECT CURRENT_REGION());
SET RUSTICE_CLOUD = (SELECT LOWER(SPLIT_PART($RUSTICE_CURRENT_REGION, '_', 1)));
SET RUSTICE_REGION_NAME = (
  SELECT LOWER(REPLACE(REGEXP_REPLACE($RUSTICE_CURRENT_REGION, '^[^_]+_', ''), '_', '-'))
);
SET RUSTICE_AWS_REGION = (
  SELECT LOWER(REPLACE(REGEXP_REPLACE($RUSTICE_CURRENT_REGION, '^AWS_', ''), '_', '-'))
);
SET RUSTICE_CATALOG_HOST = (SELECT $RUSTICE_ACCOUNT_IDENTIFIER || '.snowflakecomputing.com');
SET RUSTICE_CATALOG_URL = (SELECT 'https://' || $RUSTICE_CATALOG_HOST || '/polaris/api/catalog');
SET RUSTICE_SNOWFLAKE_ISSUER_HOST = (
  SELECT $RUSTICE_ACCOUNT_LOCATOR || '.' ||
    $RUSTICE_REGION_NAME || '.' ||
    $RUSTICE_CLOUD ||
    '.snowflakecomputing.com'
);
SET RUSTICE_S3_HOST = (SELECT 's3.' || $RUSTICE_AWS_REGION || '.amazonaws.com');
SET RUSTICE_EFFECTIVE_S3_REGION = (
  SELECT COALESCE(NULLIF($RUSTICE_S3_REGION, ''), $RUSTICE_AWS_REGION)
);
SET RUSTICE_REGISTRY_HOST = (
  SELECT $RUSTICE_ACCOUNT_IDENTIFIER || '.registry.snowflakecomputing.com'
);

SET RUSTICE_SCHEMA_FQN = (SELECT $RUSTICE_DB || '.' || $RUSTICE_SCHEMA);
SET RUSTICE_IMAGE_REPOSITORY_FQN = (
  SELECT $RUSTICE_SCHEMA_FQN || '.' || $RUSTICE_IMAGE_REPOSITORY
);
SET RUSTICE_SERVICE_FQN = (SELECT $RUSTICE_SCHEMA_FQN || '.' || $RUSTICE_SERVICE);
SET RUSTICE_SERVICE_ROLE_FQN = (
  SELECT $RUSTICE_SERVICE_FQN || '!' || $RUSTICE_SERVICE_ROLE
);
SET RUSTICE_NETWORK_RULE_FQN = (SELECT $RUSTICE_SCHEMA_FQN || '.RUSTICE_HORIZON_EGRESS');
SET RUSTICE_EAI = 'RUSTICE_HORIZON_EAI';
SET RUSTICE_JWT_SECRET = (SELECT $RUSTICE_SCHEMA_FQN || '.RUSTICE_JWT_SECRET');
SET RUSTICE_HORIZON_SECRET = (SELECT $RUSTICE_SCHEMA_FQN || '.RUSTICE_HORIZON_PAT');
SET RUSTICE_HORIZON_PAT_AUTH_POLICY = (
  SELECT $RUSTICE_SCHEMA_FQN || '.RUSTICE_HORIZON_PAT_AUTH_POLICY'
);
SET RUSTICE_INGRESS_PAT_AUTH_POLICY = (
  SELECT $RUSTICE_SCHEMA_FQN || '.RUSTICE_INGRESS_PAT_AUTH_POLICY'
);
SET RUSTICE_HORIZON_PUBLIC_SCHEMA = (
  SELECT $RUSTICE_HORIZON_DATABASE || '.PUBLIC'
);
SET RUSTICE_SERVICE_IMAGE = (
  SELECT $RUSTICE_REGISTRY_HOST || '/' ||
    LOWER($RUSTICE_DB) || '/' ||
    LOWER($RUSTICE_SCHEMA) || '/' ||
    LOWER($RUSTICE_IMAGE_REPOSITORY) ||
    '/rustice:' || $RUSTICE_IMAGE_TAG
);
SET RUSTICE_JWT_SECRET_VALUE = (SELECT UUID_STRING() || UUID_STRING());

SELECT
  $RUSTICE_SERVICE_IMAGE AS service_image,
  $RUSTICE_CATALOG_URL AS catalog_url,
  $RUSTICE_SNOWFLAKE_ISSUER_HOST AS snowflake_issuer_host,
  $RUSTICE_CATALOG_HOST || ',' || $RUSTICE_S3_HOST AS egress_hosts;

-- ---------------------------------------------------------------------------
-- 2. Base objects
-- ---------------------------------------------------------------------------
-- The image repository must exist before a Docker image can be pushed to
-- Snowflake's registry. The compute pool uses the minimum CPU_X64_XS shape.

CREATE DATABASE IF NOT EXISTS IDENTIFIER($RUSTICE_DB);
CREATE SCHEMA IF NOT EXISTS IDENTIFIER($RUSTICE_SCHEMA_FQN);
CREATE IMAGE REPOSITORY IF NOT EXISTS IDENTIFIER($RUSTICE_IMAGE_REPOSITORY_FQN);

EXECUTE IMMEDIATE $$
DECLARE
  pool_sql STRING;
BEGIN
  pool_sql := 'CREATE COMPUTE POOL IF NOT EXISTS ' || $RUSTICE_COMPUTE_POOL ||
    ' MIN_NODES = ' || $RUSTICE_POOL_MIN_NODES ||
    ' MAX_NODES = ' || $RUSTICE_POOL_MAX_NODES ||
    ' INSTANCE_FAMILY = ' || $RUSTICE_INSTANCE_FAMILY;
  EXECUTE IMMEDIATE :pool_sql;
  RETURN 'Compute pool configured: ' || $RUSTICE_COMPUTE_POOL;
END;
$$;

-- ---------------------------------------------------------------------------
-- 3. External access for Horizon REST Catalog and object storage
-- ---------------------------------------------------------------------------
-- Rustice exchanges a Snowflake PAT for a Horizon token, then Horizon vends
-- temporary object-store credentials/locations. The network rule allows the
-- Horizon account host and the regional S3 endpoint used by Snowflake-managed
-- Iceberg on AWS accounts.

CREATE OR REPLACE NETWORK RULE IDENTIFIER($RUSTICE_NETWORK_RULE_FQN)
  TYPE = HOST_PORT
  MODE = EGRESS
  VALUE_LIST = ($RUSTICE_CATALOG_HOST, $RUSTICE_S3_HOST);

EXECUTE IMMEDIATE $$
DECLARE
  eai_sql STRING;
BEGIN
  eai_sql := 'CREATE OR REPLACE EXTERNAL ACCESS INTEGRATION ' || $RUSTICE_EAI ||
    ' ALLOWED_NETWORK_RULES = (' || $RUSTICE_NETWORK_RULE_FQN || ')' ||
    ' ENABLED = TRUE';
  EXECUTE IMMEDIATE :eai_sql;
  RETURN 'External access integration created: ' || $RUSTICE_EAI;
END;
$$;

-- ---------------------------------------------------------------------------
-- 4. Horizon schema defaults for Snowflake-managed Iceberg
-- ---------------------------------------------------------------------------
-- Plain CREATE TABLE through Horizon REST does not carry Snowflake SQL clauses
-- such as EXTERNAL_VOLUME or CATALOG, so set schema defaults up front.

EXECUTE IMMEDIATE $$
BEGIN
  EXECUTE IMMEDIATE 'ALTER SCHEMA IF EXISTS ' || $RUSTICE_HORIZON_PUBLIC_SCHEMA ||
    ' SET EXTERNAL_VOLUME = ''' ||
    REPLACE($RUSTICE_HORIZON_EXTERNAL_VOLUME, '''', '''''') || '''';

  EXECUTE IMMEDIATE 'ALTER SCHEMA IF EXISTS ' || $RUSTICE_HORIZON_PUBLIC_SCHEMA ||
    ' SET CATALOG = ''' ||
    REPLACE($RUSTICE_HORIZON_CATALOG, '''', '''''') || '''';

  RETURN 'Horizon schema defaults configured: ' || $RUSTICE_HORIZON_PUBLIC_SCHEMA;
END;
$$;

-- ---------------------------------------------------------------------------
-- 5. Runtime secrets
-- ---------------------------------------------------------------------------
-- JWT_SECRET signs Embucket/Rustice session tokens. CREATE SECRET IF NOT EXISTS
-- preserves an existing value on reruns.

CREATE SECRET IF NOT EXISTS IDENTIFIER($RUSTICE_JWT_SECRET)
  TYPE = GENERIC_STRING
  SECRET_STRING = $RUSTICE_JWT_SECRET_VALUE;

-- ---------------------------------------------------------------------------
-- 6. Horizon service user and PAT secret
-- ---------------------------------------------------------------------------
-- This service user is used by the container to authenticate to Horizon REST
-- Catalog. The role restriction scopes the PAT to RUSTICE_HORIZON_ROLE.

EXECUTE IMMEDIATE $$
BEGIN
  EXECUTE IMMEDIATE 'CREATE USER IF NOT EXISTS ' || $RUSTICE_HORIZON_SERVICE_USER ||
    ' TYPE = SERVICE' ||
    ' DEFAULT_ROLE = ' || $RUSTICE_HORIZON_ROLE ||
    ' COMMENT = ''Service user used by rustice SPCS to access Horizon Catalog''';

  EXECUTE IMMEDIATE 'GRANT ROLE ' || $RUSTICE_HORIZON_ROLE ||
    ' TO USER ' || $RUSTICE_HORIZON_SERVICE_USER;

  EXECUTE IMMEDIATE 'CREATE AUTHENTICATION POLICY IF NOT EXISTS ' ||
    $RUSTICE_HORIZON_PAT_AUTH_POLICY ||
    ' PAT_POLICY = (' ||
    ' NETWORK_POLICY_EVALUATION = ENFORCED_NOT_REQUIRED' ||
    ' REQUIRE_ROLE_RESTRICTION_FOR_SERVICE_USERS = TRUE' ||
    ' )';

  EXECUTE IMMEDIATE 'ALTER USER IF EXISTS ' || $RUSTICE_HORIZON_SERVICE_USER ||
    ' SET AUTHENTICATION POLICY ' || $RUSTICE_HORIZON_PAT_AUTH_POLICY ||
    ' FORCE';

  RETURN 'Horizon service user configured: ' || $RUSTICE_HORIZON_SERVICE_USER;
END;
$$;

-- Rotate the Horizon PAT and store token_secret in a Snowflake SECRET without
-- printing the secret in the query output.
EXECUTE IMMEDIATE $$
DECLARE
  pat_token STRING;
BEGIN
  BEGIN
    EXECUTE IMMEDIATE 'ALTER USER IF EXISTS ' || $RUSTICE_HORIZON_SERVICE_USER ||
      ' REMOVE PROGRAMMATIC ACCESS TOKEN ' || $RUSTICE_HORIZON_PAT_NAME;
  EXCEPTION
    WHEN OTHER THEN
      NULL;
  END;

  EXECUTE IMMEDIATE 'ALTER USER IF EXISTS ' || $RUSTICE_HORIZON_SERVICE_USER ||
    ' ADD PROGRAMMATIC ACCESS TOKEN ' || $RUSTICE_HORIZON_PAT_NAME ||
    ' ROLE_RESTRICTION = ''' || $RUSTICE_HORIZON_ROLE || '''' ||
    ' DAYS_TO_EXPIRY = ' || $RUSTICE_HORIZON_PAT_DAYS;

  SELECT "token_secret"
    INTO :pat_token
    FROM TABLE(RESULT_SCAN(LAST_QUERY_ID()));

  EXECUTE IMMEDIATE 'CREATE OR REPLACE SECRET ' || $RUSTICE_HORIZON_SECRET ||
    ' TYPE = GENERIC_STRING SECRET_STRING = ''' ||
    REPLACE(pat_token, '''', '''''') || '''';

  RETURN 'Horizon PAT secret created in ' || $RUSTICE_HORIZON_SECRET;
END;
$$;

-- ---------------------------------------------------------------------------
-- 7. Create or replace the public SPCS service
-- ---------------------------------------------------------------------------
-- This matches deploy.sh defaults:
--   - public HTTP endpoint on port 3000
--   - executeAsCaller enabled
--   - AUTH_TRUST_SPCS_INGRESS=true
--   - SNOWFLAKE_ISSUER_HOST for SPCS caller token structural validation
--   - Horizon REST env vars and Snowflake SECRET mounts
--
-- If this fails with "image not found", push the image shown by
-- RUSTICE_SERVICE_IMAGE above, then rerun this section.

EXECUTE IMMEDIATE $$
DECLARE
  spec_yaml STRING;
  service_sql STRING;
BEGIN
  EXECUTE IMMEDIATE 'DROP SERVICE IF EXISTS ' || $RUSTICE_SERVICE_FQN;

  spec_yaml := 'spec:
  containers:
    - name: ' || $RUSTICE_CONTAINER_NAME || '
      image: ' || $RUSTICE_SERVICE_IMAGE || '
      env:
        BUCKET_HOST: "0.0.0.0"
        BUCKET_PORT: "3000"
        RUST_LOG: "info"
        AUTH_TRUST_SPCS_INGRESS: "true"
        SNOWFLAKE_ISSUER_HOST: "' || $RUSTICE_SNOWFLAKE_ISSUER_HOST || '"
        AWS_REGION: "' || $RUSTICE_EFFECTIVE_S3_REGION || '"
        AWS_DEFAULT_REGION: "' || $RUSTICE_EFFECTIVE_S3_REGION || '"
        CATALOG_URL: "' || $RUSTICE_CATALOG_URL || '"
        ICEBERG_REST_PREFIX: "' || $RUSTICE_HORIZON_DATABASE || '"
        ICEBERG_REST_CATALOG: "' || $RUSTICE_CLIENT_DATABASE || '"
        ICEBERG_REST_ACCESS_DELEGATION: "vended-credentials"
        ICEBERG_REST_SCOPE: "session:role:' || $RUSTICE_HORIZON_ROLE || '"
        ICEBERG_REST_SCHEMAS: "' || $RUSTICE_HORIZON_SCHEMAS || '"
        ICEBERG_REST_EAGER_LOAD: "' || $RUSTICE_HORIZON_EAGER_LOAD || '"
        ICEBERG_REST_TABLES: "' || $RUSTICE_HORIZON_TABLES || '"
      secrets:
        - snowflakeSecret: ' || $RUSTICE_JWT_SECRET || '
          envVarName: JWT_SECRET
          secretKeyRef: secret_string
        - snowflakeSecret: ' || $RUSTICE_HORIZON_SECRET || '
          envVarName: ICEBERG_REST_CREDENTIAL
          secretKeyRef: secret_string
      readinessProbe:
        port: 3000
        path: /health
  endpoints:
    - name: ' || $RUSTICE_ENDPOINT_NAME || '
      port: 3000
      public: true
capabilities:
  securityContext:
    executeAsCaller: true
serviceRoles:
  - name: ' || $RUSTICE_SERVICE_ROLE || '
    endpoints:
      - ' || $RUSTICE_ENDPOINT_NAME || '
';

  service_sql := 'CREATE SERVICE ' || $RUSTICE_SERVICE_FQN ||
    ' IN COMPUTE POOL ' || $RUSTICE_COMPUTE_POOL ||
    ' FROM SPECIFICATION ''' || REPLACE(spec_yaml, '''', '''''') || '''' ||
    ' AUTO_SUSPEND_SECS = 0' ||
    ' EXTERNAL_ACCESS_INTEGRATIONS = (' || $RUSTICE_EAI || ')' ||
    ' AUTO_RESUME = TRUE' ||
    ' MIN_INSTANCES = 1' ||
    ' MAX_INSTANCES = 1';

  EXECUTE IMMEDIATE :service_sql;
  RETURN 'Service created: ' || $RUSTICE_SERVICE_FQN;
END;
$$;

-- ---------------------------------------------------------------------------
-- 8. Ingress-only user/PAT for embucket-snow
-- ---------------------------------------------------------------------------
-- This PAT is not used by the container. It is used by the client to pass SPCS
-- public ingress. SQL cannot write a local token file, so the final result
-- prints token_secret once. Store it in a local file with chmod/umask 077.

EXECUTE IMMEDIATE $$
BEGIN
  EXECUTE IMMEDIATE 'CREATE ROLE IF NOT EXISTS ' || $RUSTICE_INGRESS_ROLE;

  EXECUTE IMMEDIATE 'CREATE USER IF NOT EXISTS ' || $RUSTICE_INGRESS_SERVICE_USER ||
    ' TYPE = SERVICE' ||
    ' DEFAULT_ROLE = ' || $RUSTICE_INGRESS_ROLE ||
    ' COMMENT = ''Service user used by embucket-snow to access Rustice SPCS ingress''';

  EXECUTE IMMEDIATE 'GRANT ROLE ' || $RUSTICE_INGRESS_ROLE ||
    ' TO USER ' || $RUSTICE_INGRESS_SERVICE_USER;

  EXECUTE IMMEDIATE 'GRANT USAGE ON DATABASE ' || $RUSTICE_DB ||
    ' TO ROLE ' || $RUSTICE_INGRESS_ROLE;

  EXECUTE IMMEDIATE 'GRANT USAGE ON SCHEMA ' || $RUSTICE_SCHEMA_FQN ||
    ' TO ROLE ' || $RUSTICE_INGRESS_ROLE;

  EXECUTE IMMEDIATE 'CREATE AUTHENTICATION POLICY IF NOT EXISTS ' ||
    $RUSTICE_INGRESS_PAT_AUTH_POLICY ||
    ' PAT_POLICY = (' ||
    ' NETWORK_POLICY_EVALUATION = ENFORCED_NOT_REQUIRED' ||
    ' REQUIRE_ROLE_RESTRICTION_FOR_SERVICE_USERS = TRUE' ||
    ' )';

  EXECUTE IMMEDIATE 'ALTER USER IF EXISTS ' || $RUSTICE_INGRESS_SERVICE_USER ||
    ' SET AUTHENTICATION POLICY ' || $RUSTICE_INGRESS_PAT_AUTH_POLICY ||
    ' FORCE';

  RETURN 'Ingress service user configured: ' || $RUSTICE_INGRESS_SERVICE_USER;
END;
$$;

EXECUTE IMMEDIATE $$
DECLARE
  ingress_pat_token STRING;
BEGIN
  EXECUTE IMMEDIATE 'GRANT SERVICE ROLE ' || $RUSTICE_SERVICE_ROLE_FQN ||
    ' TO ROLE ' || $RUSTICE_INGRESS_ROLE;

  BEGIN
    EXECUTE IMMEDIATE 'ALTER USER IF EXISTS ' || $RUSTICE_INGRESS_SERVICE_USER ||
      ' REMOVE PROGRAMMATIC ACCESS TOKEN ' || $RUSTICE_INGRESS_PAT_NAME;
  EXCEPTION
    WHEN OTHER THEN
      NULL;
  END;

  EXECUTE IMMEDIATE 'ALTER USER IF EXISTS ' || $RUSTICE_INGRESS_SERVICE_USER ||
    ' ADD PROGRAMMATIC ACCESS TOKEN ' || $RUSTICE_INGRESS_PAT_NAME ||
    ' ROLE_RESTRICTION = ''' || $RUSTICE_INGRESS_ROLE || '''' ||
    ' DAYS_TO_EXPIRY = ' || $RUSTICE_INGRESS_PAT_DAYS;

  SELECT "token_secret"
    INTO :ingress_pat_token
    FROM TABLE(RESULT_SCAN(LAST_QUERY_ID()));

  RETURN OBJECT_CONSTRUCT(
    'token_file', 'embucket_spcs_token',
    'token_secret', ingress_pat_token,
    'next_step', 'Copy token_secret into a local file named embucket_spcs_token next to your embucket-snow config.toml.'
  );
END;
$$;

-- ---------------------------------------------------------------------------
-- 9. Inspect deployment
-- ---------------------------------------------------------------------------
-- Wait until SYSTEM$GET_SERVICE_STATUS shows READY and SHOW ENDPOINTS returns
-- an ingress_url ending in .snowflakecomputing.app. Use that host in the
-- embucket-snow config.

SHOW SERVICES IN SCHEMA IDENTIFIER($RUSTICE_SCHEMA_FQN);
SELECT SYSTEM$GET_SERVICE_STATUS($RUSTICE_SERVICE_FQN) AS service_status;
SHOW ENDPOINTS IN SERVICE IDENTIFIER($RUSTICE_SERVICE_FQN);
SHOW SERVICE CONTAINERS IN SERVICE IDENTIFIER($RUSTICE_SERVICE_FQN);

-- Local client config shape for embucket-snow:
--
-- default_connection_name = "embucket_spcs"
--
-- [connections.embucket_spcs]
-- host = "<ingress_url from SHOW ENDPOINTS>"
-- protocol = "https"
-- port = 443
-- account = "embucket"
-- user = "embucket"
-- password = "embucket"
-- database = "rustice_spcs"
-- schema = "public"
-- warehouse = "embucket"
-- spcs_token_connection = "<regular Snowflake CLI profile>"
-- spcs_token_config_file = "/path/to/config.toml"
--
-- The spcs_token_connection profile issues short-lived SPCS ingress tokens in
-- memory. The role used by that profile must be granted the service role above.
-- As a fallback, put the token_secret returned in section 8 into a local file
-- next to this config:
--   umask 077
--   printf '%s' '<token_secret>' > embucket_spcs_token
--
-- Smoke query:
--   embucket-snow --config-file ./config.toml sql -c embucket_spcs \
--     -q "SELECT * FROM rustice_spcs.public.smoke"
