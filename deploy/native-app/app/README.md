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

Then start the service:

```sql
CALL <app_name>.APP_PUBLIC.START_APP();
CALL <app_name>.APP_PUBLIC.SERVICE_STATUS();
CALL <app_name>.APP_PUBLIC.SERVICE_ENDPOINTS();
```

Use the endpoint host returned by `SERVICE_ENDPOINTS()` with the
`embucket-snow` CLI or dbt Snowflake adapter patch.

The current Horizon auth path is experimental. If reads from
Snowflake-managed Iceberg tables return `401 Unauthorized`, use the next
package version with the consumer-approved Horizon credential/reference flow.
