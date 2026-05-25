# Rustice Native App

Rustice runs a Snowflake-compatible SQL endpoint inside Snowpark Container
Services. It is designed to execute Snowflake/dbt-style SQL over
Snowflake-managed Iceberg tables through Horizon/Snowflake REST Catalog.

## Configure

After installing the app, grant the requested privileges and configure external
access:

```sql
CALL <app_name>.APP_PUBLIC.CONFIGURE_EXTERNAL_ACCESS(
  'RUSTICE_SPCS',
  '<horizon_role>',
  'rustice_spcs',
  'public',
  'PUBLIC,public',
  '',
  '',
  ''
);
```

Approve the external access request in Snowsight if prompted, or approve it
with SQL:

```sql
SHOW SPECIFICATIONS IN APPLICATION <app_name>;

ALTER APPLICATION <app_name>
  APPROVE SPECIFICATION RUSTICE_EXTERNAL_ACCESS
  SEQUENCE_NUMBER = <sequence_number>;
```

Create or select a Snowflake `GENERIC_STRING` secret containing a
Horizon-compatible credential, then bind it to the app reference:

```sql
SHOW REFERENCES IN APPLICATION <app_name>;

CALL <app_name>.APP_PUBLIC.REGISTER_REFERENCE(
  'horizon_credential_secret',
  'ADD',
  SYSTEM$REFERENCE('SECRET', '<db>.<schema>.<secret>', 'PERSISTENT', 'READ')
);
```

Then start the service:

```sql
CALL <app_name>.APP_PUBLIC.START_APP();
CALL <app_name>.APP_PUBLIC.SERVICE_STATUS();
CALL <app_name>.APP_PUBLIC.SERVICE_ENDPOINTS();
CALL <app_name>.APP_PUBLIC.SERVICE_LOGS(100);
CALL <app_name>.APP_PUBLIC.SERVICE_PREVIOUS_LOGS(100);
```

Use the endpoint host returned by `SERVICE_ENDPOINTS()` with the
`embucket-snow` CLI or dbt Snowflake adapter patch.

The service mounts the bound secret as a file and points Rustice to it with
`ICEBERG_REST_CREDENTIAL_FILE`; Rustice uses that credential to exchange and
refresh Horizon/Snowflake REST Catalog access tokens.

The `<horizon_role>` argument must match the PAT `ROLE_RESTRICTION` used in the
bound secret, otherwise Horizon returns `unauthorized_client`.
