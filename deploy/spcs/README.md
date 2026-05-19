# Deploy Rustice to Snowpark Container Services

This directory deploys the `embucketd` container from `rustice` to Snowpark Container Services (SPCS) by using Snowflake CLI SQL commands.

## Image

`rustice` already has:

- A root `Dockerfile` that builds `embucketd`.
- A release workflow that publishes Docker Hub image `embucket/rustice`.

The deploy script uses `embucket/rustice:latest` by default. Set `RUSTICE_BUILD_LOCAL=1` to build the local checkout instead.

SPCS currently requires `linux/amd64` images, so the script uses that platform when it pulls or builds the image.

The normal user path is to use the Docker Hub image published by the repository release workflow. The script still copies that image into a Snowflake image repository, because SPCS services run images from Snowflake's registry:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=ANALYTICS \
RUSTICE_HORIZON_ROLE=DATA_ENGINEER \
RUSTICE_IMAGE_TAG=latest \
./deploy/spcs/deploy.sh
```

Use `RUSTICE_BUILD_LOCAL=1` only while testing changes from a local checkout.

## Quick Start

Run with a Snowflake CLI connection that can create SPCS resources, external access integrations, service users, and PATs:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=ANALYTICS \
RUSTICE_HORIZON_ROLE=DATA_ENGINEER \
./deploy/spcs/deploy.sh
```

After the service is ready, the script creates `deploy/spcs/generated/config.toml` for the patched `embucket-snow` CLI. The smoke command printed by the script can be run directly:

```bash
embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "SELECT * FROM embucket.public.smoke"
```

The default mode is `RUSTICE_HORIZON_AUTH=pat`:

1. Creates a `TYPE = SERVICE` user.
2. Grants `RUSTICE_HORIZON_ROLE` to that user.
3. Generates a role-restricted programmatic access token (PAT).
4. Stores the PAT in a Snowflake `SECRET`.
5. Mounts the secret into the SPCS container as `ICEBERG_REST_CREDENTIAL`.

`rustice` exchanges that credential for a Horizon Catalog access token at startup and uses `ICEBERG_REST_PREFIX` as the Horizon database/prefix.
The SQL catalog name exposed by Rustice remains `embucket`; the Horizon database/prefix is configured separately through `RUSTICE_HORIZON_DATABASE`.

## Deployment Modes

There are three supported ways to create the SPCS resources:

1. Run `deploy.sh`. This is the easiest path because it builds or pulls the image, logs in to the Snowflake image registry, pushes the image into Snowflake, creates the compute pool, secrets, EAI, service, and grants.
2. Run [deploy.sql](deploy.sql). This is a pure SQL template with comments for every block and the same default object names as `deploy.sh`. Edit section `0. Parameters`, make sure the image already exists in the Snowflake image repository, then run it in Snowsight or with `snow sql`.
3. Generate SQL with `RUSTICE_DRY_RUN=1` and run that SQL manually in Snowsight or through `snow sql`. This is useful when a user wants to review or adapt the exact DDL produced by the shell script. The image must still exist in a Snowflake image repository before the service can start; use `RUSTICE_SKIP_IMAGE_PUSH=1` only after the image has already been pushed.

After deployment, Snowflake SQL is used to manage and inspect the SPCS service. SQL execution against Embucket/Rustice itself goes through the Snowflake-compatible REST endpoint exposed by the SPCS public ingress.

## SQL-Only Deployment

Use [deploy.sql](deploy.sql) when you want a worksheet-friendly deployment without running the shell script:

```bash
snow --config-file /path/to/config.toml \
  sql -c snowflake \
  --filename deploy/spcs/deploy.sql
```

Before running it:

- Set `RUSTICE_HORIZON_DATABASE` to the Snowflake database that contains the Snowflake-managed Iceberg tables.
- Set `RUSTICE_HORIZON_ROLE` to the role that should access those Iceberg tables through Horizon.
- Push `embucket/rustice:<tag>` into the Snowflake image repository named by `RUSTICE_DB`, `RUSTICE_SCHEMA`, and `RUSTICE_IMAGE_REPOSITORY`.

Pure SQL cannot pull, tag, or push Docker images and cannot write local client config files. The SQL template creates the image repository, but the image must be pushed separately before `CREATE SERVICE` can start the container.

The final PAT block returns an ingress `token_secret` once. Treat it as a secret, then write it next to the `embucket-snow` config:

```bash
umask 077
printf '%s' '<token_secret>' > embucket_spcs_token
```

Use the `ingress_url` from `SHOW ENDPOINTS IN SERVICE` as the `host` in the `embucket_spcs` connection profile.

## Common Options

```bash
RUSTICE_DB=RUSTICE_APP
RUSTICE_SCHEMA=PUBLIC
RUSTICE_COMPUTE_POOL=RUSTICE_POOL
RUSTICE_INSTANCE_FAMILY=CPU_X64_XS
RUSTICE_IMAGE_TAG=latest
RUSTICE_REGISTRY_LOGIN=1
RUSTICE_CREATE_PAT_AUTH_POLICY=1
RUSTICE_HORIZON_SCHEMAS=PUBLIC,public
RUSTICE_HORIZON_TABLES=PUBLIC.SMOKE
RUSTICE_HORIZON_EAGER_LOAD=0
RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS=1
RUSTICE_HORIZON_EXTERNAL_VOLUME=SNOWFLAKE_MANAGED
RUSTICE_HORIZON_CATALOG=SNOWFLAKE
RUSTICE_TRUST_SPCS_INGRESS=1
RUSTICE_CREATE_INGRESS_PAT=1
RUSTICE_GENERATE_CLIENT_CONFIG=1
RUSTICE_CLIENT_OUTPUT_DIR=deploy/spcs/generated
RUSTICE_CLIENT_TOKEN_CONNECTION=<SNOW_CONNECTION>
RUSTICE_CLIENT_TOKEN_CONFIG_FILE=<SNOW_CONFIG_FILE>
RUSTICE_WAIT_FOR_READY=1
RUSTICE_EGRESS_HOSTS=<optional-comma-separated-egress-hosts>
RUSTICE_GRANT_TO_ROLE=ANALYST
RUSTICE_AUTO_SUSPEND_SECS=0
RUSTICE_BUILD_LOCAL=1
RUSTICE_DRY_RUN=1
```

When `RUSTICE_GRANT_TO_ROLE` is set, the script grants that role `USAGE` on the service database/schema and grants the service role for the public endpoint.

The script uses `snow spcs image-registry login` by default before pushing the image. Docker or Podman must be available and running locally; in WSL, Docker Desktop WSL integration must be enabled. Set `RUSTICE_REGISTRY_LOGIN=0` only if the container CLI is already logged in to the Snowflake registry.

Use `RUSTICE_HORIZON_AUTH=none` to deploy only the service shell without Horizon credentials.

Use `RUSTICE_HORIZON_AUTH=bearer_token` or `oauth_token` if you already manage a Snowflake `SECRET` containing a token. In that case set `RUSTICE_HORIZON_SECRET=<db>.<schema>.<secret>`.

The deploy script enables `RUSTICE_TRUST_SPCS_INGRESS=1` by default. In that mode, `/session/v1/login-request` trusts Snowflake SPCS public ingress authentication and does not require the demo Embucket password. Snowflake injects `Sf-Context-Current-User` into the request after ingress authentication, and Rustice records that value in session metadata for future caller-aware checks. Set `RUSTICE_TRUST_SPCS_INGRESS=0` only for compatibility testing where you still want `AUTH_DEMO_USER`/`AUTH_DEMO_PASSWORD` login checks.

By default, Rustice bootstraps the REST catalog lazily with `RUSTICE_HORIZON_SCHEMAS=PUBLIC,public` and `RUSTICE_HORIZON_EAGER_LOAD=0`. This avoids startup failures in Horizon environments that allow direct table access but restrict broad namespace/table listing. Set `RUSTICE_HORIZON_TABLES` to a comma-separated list such as `PUBLIC.SMOKE,ANALYTICS.ORDERS` when you need existing tables to be visible without eager listing. Set `RUSTICE_HORIZON_EAGER_LOAD=1` when the Horizon role is allowed to list all namespaces and tables during startup.

By default, the script also configures Iceberg defaults for each schema listed in `RUSTICE_HORIZON_SCHEMAS`:

```sql
ALTER SCHEMA <horizon_database>.<schema> SET EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';
ALTER SCHEMA <horizon_database>.<schema> SET CATALOG = 'SNOWFLAKE';
```

This is needed for plain `CREATE TABLE` statements sent through Rustice to Horizon REST Catalog, because the REST create request does not carry Snowflake SQL clauses such as `EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED'`. Set `RUSTICE_CONFIGURE_HORIZON_SCHEMA_DEFAULTS=0` if those schema defaults are managed separately. For a custom external volume, set `RUSTICE_HORIZON_EXTERNAL_VOLUME=<volume_name>` and grant `USAGE` on that external volume to `RUSTICE_HORIZON_ROLE`.

Snowflake requires service users to satisfy programmatic access token policy requirements before a PAT can be generated. The default `RUSTICE_CREATE_PAT_AUTH_POLICY=1` creates a user-scoped authentication policy with `NETWORK_POLICY_EVALUATION = ENFORCED_NOT_REQUIRED`. Set `RUSTICE_CREATE_PAT_AUTH_POLICY=0` if your account already enforces a suitable network or authentication policy for the service user.

The deploy script also creates an ingress-only service user/PAT by default with `RUSTICE_CREATE_INGRESS_PAT=1`. That PAT is written to `deploy/spcs/generated/embucket_spcs_token` with local user-only permissions as a fallback for non-interactive environments. The normal `embucket-snow` path does not need that file: it uses the regular Snowflake CLI profile named by `RUSTICE_CLIENT_TOKEN_CONNECTION` to issue a short-lived SPCS ingress token in memory for each CLI process.

`RUSTICE_GENERATE_CLIENT_CONFIG=1` writes `deploy/spcs/generated/config.toml` with an `embucket_spcs` profile that points at the public ingress URL and includes `spcs_token_connection = "<SNOW_CONNECTION>"`. When `SNOW_CONFIG_FILE` is provided, the generated profile also includes `spcs_token_config_file = "<SNOW_CONFIG_FILE>"`. This lets the standard smoke command work without extra environment variables or token-file rotation. Set `RUSTICE_GENERATE_CLIENT_CONFIG=0` when client config is managed outside the deploy script.

If your environment uses short-lived OAuth tokens instead of the generated PAT file, set `EMBUCKET_SPCS_TOKEN_COMMAND=/path/to/get-spcs-token` when running `embucket-snow`. The command is executed without a shell and may return either a raw token or a full `Snowflake Token="..."` header value.

The service uses a public SPCS endpoint for the Snowflake-compatible ingress. Snowflake does not support service auto-suspend for public endpoints, so `RUSTICE_AUTO_SUSPEND_SECS` must be `0`.

`RUSTICE_CATALOG_URL` defaults to the Snowflake Horizon Catalog endpoint resolved from the active Snowflake CLI connection:

```sql
SELECT LOWER(REPLACE(CURRENT_ORGANIZATION_NAME() || '-' || CURRENT_ACCOUNT_NAME(), '_', '-'));
```

The default URL is:

```text
https://<org>-<account>.snowflakecomputing.com/polaris/api/catalog
```

`RUSTICE_EGRESS_HOSTS` controls the External Access Integration allowlist. By default, the script includes the Horizon catalog host. On AWS accounts it also derives `s3.<region>.amazonaws.com` from `CURRENT_REGION()` for Snowflake-managed Iceberg metadata and Parquet reads. Override `RUSTICE_EGRESS_HOSTS` only when Horizon returns an object-store or redirect host that is not covered by the default allowlist.

In `RUSTICE_DRY_RUN=1` mode, the script does not call Snowflake to resolve account metadata, so it uses placeholder values such as `example-org-example-account.snowflakecomputing.com`. To generate account-specific dry-run SQL, pass the resolved values explicitly:

```bash
RUSTICE_DRY_RUN=1 \
RUSTICE_ACCOUNT_IDENTIFIER=<org>-<account> \
RUSTICE_CURRENT_REGION=AWS_US_EAST_2 \
./deploy/spcs/deploy.sh
```

## Public Endpoint Authentication

SPCS public ingress authenticates programmatic requests with the standard Snowflake token header:

```http
Authorization: Snowflake Token="<pat-or-oauth-token>"
```

Snowflake's ingress proxy validates that token before the request reaches the container. With the default deploy setting `AUTH_TRUST_SPCS_INGRESS=true`, Rustice treats successful ingress as the client authentication boundary and derives its internal session from Snowflake's caller context headers:

- `Sf-Context-Current-User-Token` when Snowflake provides it. Rustice structurally validates the SCT claims (`type`, `exp`, `aud`, `iss`, `callContext`, `sub`) and uses stable claims from that token for the session identity.
- `Sf-Context-Current-Account` plus `Sf-Context-Current-User` as the fallback session identity when the SCT header is not present.

The Snowflake-compatible `/session/v1/login-request` still returns `data.token`, but in SPCS mode that value is only an opaque server-side session id. Clients keep sending the Snowflake ingress token in the standard `Authorization` header on login, query, and result requests. No `X-Embucket-Authorization` header is required.

The password field in the Snowflake-compatible login payload is ignored in this mode, and the Snowflake caller user is recorded from `Sf-Context-Current-User`. `AUTH_TRUST_SPCS_INGRESS=true` must only be used behind Snowflake SPCS public ingress; do not expose a service with this setting directly on an untrusted network, because caller context headers can be forged outside Snowflake ingress.

Rustice server-side sessions use a 4 hour sliding inactivity window. `/session/heartbeat` refreshes that window, matching the way Snowflake-compatible drivers keep active sessions alive.

When using PATs for programmatic SPCS ingress access, Snowflake requires the PAT user to have a network policy. Browser/OAuth access can be used instead for interactive checks.

By default the deploy script creates this PAT automatically:

```sql
CREATE ROLE RUSTICE_INGRESS_ROLE;
CREATE USER RUSTICE_INGRESS_SVC TYPE = SERVICE DEFAULT_ROLE = RUSTICE_INGRESS_ROLE;
GRANT SERVICE ROLE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE!RUSTICE_USER TO ROLE RUSTICE_INGRESS_ROLE;
ALTER USER RUSTICE_INGRESS_SVC ADD PROGRAMMATIC ACCESS TOKEN RUSTICE_INGRESS_PAT
  ROLE_RESTRICTION = 'RUSTICE_INGRESS_ROLE'
  DAYS_TO_EXPIRY = 1;
```

The returned token secret is written to the generated token file instead of being printed in logs.

## Query Through the SPCS Endpoint

Embucket/Rustice exposes the same Snowflake-compatible REST flow that the Snowflake CLI/connector uses:

1. `/session/v1/login-request` creates an Embucket/Rustice server-side session and returns `data.token`.
2. `/queries/v1/query-request` executes SQL in that session.
3. `/queries/{query_id}/result` fetches async result chunks when needed.

Behind SPCS public ingress, the client must authenticate to Snowflake ingress on every request. A Snowflake-compatible CLI or connector can query the SPCS endpoint if it is configured to:

- point the Snowflake host/account URL at the SPCS public endpoint;
- keep `Authorization: Snowflake Token="<issued-token>"` on every request for SPCS ingress.

The `embucket-snow` wrapper preserves the normal Snowflake CLI UX while keeping the Snowflake ingress token in `Authorization`. By default it logs in through the regular Snowflake profile referenced by `spcs_token_connection`, calls the Python connector's token issue flow, caches the short-lived token in memory, and sends SQL to the SPCS endpoint. The generated PAT file remains a fallback for service-user or offline token-management setups.

With the generated deploy output, the user-facing command is:

```bash
embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "SELECT * FROM embucket.public.smoke"
```

## Result

The script ends with:

```sql
SHOW SERVICES IN SCHEMA <db>.<schema>;
SELECT SYSTEM$GET_SERVICE_STATUS('<db>.<schema>.<service>');
SHOW ENDPOINTS IN SERVICE <db>.<schema>.<service>;
```

## Inspect Services and Containers

Use Snowflake SQL to inspect SPCS resources:

```sql
SHOW COMPUTE POOLS;
SHOW IMAGE REPOSITORIES IN SCHEMA RUSTICE_APP.PUBLIC;
SHOW IMAGES IN IMAGE REPOSITORY RUSTICE_APP.PUBLIC.RUSTICE_REPO;

SHOW SERVICES IN SCHEMA RUSTICE_APP.PUBLIC;
DESCRIBE SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
SHOW SERVICE INSTANCES IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
SHOW SERVICE CONTAINERS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
SHOW ENDPOINTS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
```

To inspect logs from the running container:

```sql
SELECT SYSTEM$GET_SERVICE_LOGS('RUSTICE_APP.PUBLIC.RUSTICE_SERVICE', 0, 'rustice', 100);
```

The equivalent Snowflake CLI commands are:

```bash
snow spcs compute-pool list -c snowflake
snow spcs image-repository list -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs image-repository list-images RUSTICE_REPO -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs service list -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs service describe RUSTICE_SERVICE -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs service list-instances RUSTICE_SERVICE -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs service list-containers RUSTICE_SERVICE -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs service list-endpoints RUSTICE_SERVICE -c snowflake --database RUSTICE_APP --schema PUBLIC
snow spcs service logs RUSTICE_SERVICE --container-name rustice --instance-id 0 -c snowflake --database RUSTICE_APP --schema PUBLIC
```

## Stop Service and Compute

Suspend the SPCS service and compute pool when you finish testing. This stops the running container instances and avoids leaving the `CPU_X64_XS` pool active:

```sql
ALTER SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE SUSPEND;
ALTER COMPUTE POOL RUSTICE_POOL SUSPEND;
```

The same commands through Snowflake CLI:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "ALTER SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE SUSPEND"

snow --config-file /path/to/config.toml sql -c snowflake \
  -q "ALTER COMPUTE POOL RUSTICE_POOL SUSPEND"
```

Check that nothing is running:

```sql
SHOW COMPUTE POOLS LIKE 'RUSTICE_POOL';
SHOW SERVICES LIKE 'RUSTICE_SERVICE' IN SCHEMA RUSTICE_APP.PUBLIC;
SHOW SERVICE CONTAINERS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
```

Resume for another test run:

```sql
ALTER COMPUTE POOL RUSTICE_POOL RESUME;
ALTER SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE RESUME;
```

Optional full cleanup for disposable test environments:

```sql
DROP SERVICE IF EXISTS RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
DROP COMPUTE POOL IF EXISTS RUSTICE_POOL;
DROP EXTERNAL ACCESS INTEGRATION IF EXISTS RUSTICE_HORIZON_EAI;
DROP NETWORK RULE IF EXISTS RUSTICE_APP.PUBLIC.RUSTICE_HORIZON_EGRESS;
DROP SECRET IF EXISTS RUSTICE_APP.PUBLIC.RUSTICE_HORIZON_PAT;
DROP SECRET IF EXISTS RUSTICE_APP.PUBLIC.RUSTICE_JWT_SECRET;
DROP AUTHENTICATION POLICY IF EXISTS RUSTICE_APP.PUBLIC.RUSTICE_HORIZON_PAT_AUTH_POLICY;
DROP USER IF EXISTS RUSTICE_HORIZON_SVC;
```

## Monitor Cost and Iceberg Tables

Use the account usage view for SPCS compute-pool credits:

```sql
SELECT
  START_TIME,
  END_TIME,
  COMPUTE_POOL_NAME,
  CREDITS_USED
FROM SNOWFLAKE.ACCOUNT_USAGE.SNOWPARK_CONTAINER_SERVICES_HISTORY
WHERE COMPUTE_POOL_NAME = 'RUSTICE_POOL'
  AND START_TIME >= DATEADD('day', -7, CURRENT_TIMESTAMP())
ORDER BY START_TIME DESC;
```

For account-level metering, query the general metering view:

```sql
SELECT
  START_TIME,
  END_TIME,
  SERVICE_TYPE,
  CREDITS_USED
FROM SNOWFLAKE.ACCOUNT_USAGE.METERING_HISTORY
WHERE SERVICE_TYPE = 'SNOWPARK_CONTAINER_SERVICES'
  AND START_TIME >= DATEADD('day', -7, CURRENT_TIMESTAMP())
ORDER BY START_TIME DESC;
```

Account usage views can lag by up to a few hours. For current state, use:

```sql
SHOW COMPUTE POOLS LIKE 'RUSTICE_POOL';
SHOW SERVICES LIKE 'RUSTICE_SERVICE' IN SCHEMA RUSTICE_APP.PUBLIC;
SHOW SERVICE CONTAINERS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE;
```

List Iceberg tables that the current Snowflake role can see:

```sql
SHOW ICEBERG TABLES IN DATABASE RUSTICE_E2E;
SHOW ICEBERG TABLES IN SCHEMA RUSTICE_E2E.PUBLIC;
DESCRIBE ICEBERG TABLE RUSTICE_E2E.PUBLIC.SMOKE;
```

To run the deployment from Snowsight instead of Snowflake CLI, first push the image into the Snowflake image repository once. Then either run [deploy.sql](deploy.sql) in a worksheet or run the script with `RUSTICE_SKIP_IMAGE_PUSH=1 RUSTICE_DRY_RUN=1` and paste the emitted SQL into a worksheet.

## End-to-End Smoke Test

First create a Snowflake-managed Iceberg table in the Horizon database that Rustice will use as `ICEBERG_REST_PREFIX`:

```sql
CREATE DATABASE IF NOT EXISTS RUSTICE_E2E;
CREATE SCHEMA IF NOT EXISTS RUSTICE_E2E.PUBLIC;
ALTER SCHEMA RUSTICE_E2E.PUBLIC SET EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';
ALTER SCHEMA RUSTICE_E2E.PUBLIC SET CATALOG = 'SNOWFLAKE';

CREATE OR REPLACE ICEBERG TABLE RUSTICE_E2E.PUBLIC.SMOKE (
  ID INT,
  MSG STRING
)
  CATALOG = 'SNOWFLAKE'
  EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';

INSERT INTO RUSTICE_E2E.PUBLIC.SMOKE VALUES (1, 'ok');

CREATE ROLE IF NOT EXISTS RUSTICE_E2E_ROLE;
GRANT USAGE ON DATABASE RUSTICE_E2E TO ROLE RUSTICE_E2E_ROLE;
GRANT MONITOR ON DATABASE RUSTICE_E2E TO ROLE RUSTICE_E2E_ROLE;
GRANT USAGE ON SCHEMA RUSTICE_E2E.PUBLIC TO ROLE RUSTICE_E2E_ROLE;
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON TABLE RUSTICE_E2E.PUBLIC.SMOKE TO ROLE RUSTICE_E2E_ROLE;
GRANT CREATE TABLE ON SCHEMA RUSTICE_E2E.PUBLIC TO ROLE RUSTICE_E2E_ROLE;
GRANT CREATE ICEBERG TABLE ON SCHEMA RUSTICE_E2E.PUBLIC TO ROLE RUSTICE_E2E_ROLE;
```

For Horizon write/create checks, Snowflake also requires the write path to be enabled for the account and the role to satisfy Horizon write privileges. In particular, creating an Iceberg table through Horizon requires `CREATE ICEBERG TABLE` on the schema and `USAGE` on the external volume used by the table. Grant external-volume access with your account-specific volume name when applicable:

```sql
GRANT USAGE ON EXTERNAL VOLUME <external_volume_name> TO ROLE RUSTICE_E2E_ROLE;
```

Deploy Rustice with:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_E2E \
RUSTICE_HORIZON_ROLE=RUSTICE_E2E_ROLE \
RUSTICE_INSTANCE_FAMILY=CPU_X64_XS \
RUSTICE_POOL_MIN_NODES=1 \
RUSTICE_POOL_MAX_NODES=1 \
RUSTICE_MIN_INSTANCES=1 \
RUSTICE_MAX_INSTANCES=1 \
RUSTICE_AUTO_SUSPEND_SECS=0 \
RUSTICE_HORIZON_SCHEMAS=PUBLIC,public \
RUSTICE_HORIZON_TABLES=PUBLIC.SMOKE \
./deploy/spcs/deploy.sh
```

If your Horizon/object-store host is not covered by the automatically generated EAI allowlist, rerun with an explicit override:

```bash
RUSTICE_EGRESS_HOSTS=<catalog-host>,<object-store-host> ./deploy/spcs/deploy.sh
```

Verify the baseline Snowflake-managed Iceberg table through regular Snowflake SQL. This query uses Snowflake compute and validates that the source table exists and is readable by the active Snowflake role:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SELECT * FROM RUSTICE_E2E.PUBLIC.SMOKE"
```

Expected result:

```text
+----------+
| ID | MSG |
|----+-----|
| 1  | ok  |
+----------+
```

Verify that the SPCS service is running:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW SERVICES LIKE 'RUSTICE_SERVICE' IN SCHEMA RUSTICE_APP.PUBLIC"

snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW SERVICE CONTAINERS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE"

snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW ENDPOINTS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE"
```

After the service reaches `READY`, run this SQL through the Embucket/Rustice Snowflake-compatible endpoint with the patched client/connector. The SQL catalog name remains `embucket`; `RUSTICE_E2E` is the underlying Horizon prefix:

```sql
SHOW DATABASES;
SHOW SCHEMAS IN DATABASE embucket;
SHOW TABLES IN SCHEMA embucket.public;
SELECT * FROM embucket.public.smoke;
```

If Horizon write access is enabled and the role has the required write privileges, use a separate write smoke:

```sql
CREATE TABLE embucket.public.rustice_write_smoke (
  id INT,
  msg STRING
);

INSERT INTO embucket.public.rustice_write_smoke VALUES (2, 'written by rustice');
SELECT * FROM embucket.public.rustice_write_smoke;
DROP TABLE embucket.public.rustice_write_smoke;
```

To inspect a table created through Rustice from regular Snowflake SQL or Snowsight, query the underlying Horizon database and schema. Current Rustice REST create behavior preserves the lower-case table name, so quote the table identifier in Snowflake SQL:

```sql
SHOW ICEBERG TABLES LIKE 'rustice_write_smoke' IN SCHEMA RUSTICE_E2E.PUBLIC;
SELECT * FROM RUSTICE_E2E.PUBLIC."rustice_write_smoke";
```

Snowflake SQL can manage and inspect the SPCS service directly, but it does not speak the Snowflake REST session protocol that Rustice exposes. For SQL execution against Rustice itself, use the modified Snowflake-compatible client or connector pointed at the SPCS ingress endpoint.

## Security Notes

- No local RSA keypair is required for the default path.
- The PAT is role-restricted and stored as a Snowflake `SECRET`; it is not written to the repo or printed by the deploy script.
- The SPCS service is created with `executeAsCaller: true`, so Snowflake ingress passes caller context headers to the container.
- `AUTH_TRUST_SPCS_INGRESS=true` must only be used behind Snowflake SPCS public ingress. Do not expose a service with this setting directly on an untrusted network, because caller context headers can be forged outside Snowflake ingress.
- Horizon Catalog calls are still made with the service user's role. Grant that role only the Iceberg table privileges this service is allowed to exercise.
- Horizon access tokens exchanged from the service-user credential are cached and refreshed automatically before expiration.

References:

- SPCS overview: https://docs.snowflake.com/en/developer-guide/snowpark-container-services/overview
- SPCS service specification: https://docs.snowflake.com/en/developer-guide/snowpark-container-services/specification-reference
- Horizon Catalog for Iceberg tables: https://docs.snowflake.com/en/user-guide/tables-iceberg-access-using-external-query-engine-snowflake-horizon
- Programmatic access tokens: https://docs.snowflake.com/en/user-guide/programmatic-access-tokens
