# Deploy Rustice to Snowpark Container Services

This directory deploys the `embucketd` container from `rustice` to Snowpark Container Services (SPCS) by using Snowflake CLI SQL commands.

## Prerequisites

- Snowflake CLI is installed and has a working connection profile, for example `snowflake`.
- The Snowflake role used by that profile can create SPCS resources: database/schema, image repository, compute pool, external access integration, secrets, service users, PATs, and services.
- Docker or Podman is running locally. The script copies the selected image into the Snowflake image registry because SPCS runs images from Snowflake's registry.
- `embucket-snow` is installed from [Embucket/embucket-snowflake-connector](https://github.com/Embucket/embucket-snowflake-connector). It uses the generated config to query Rustice through the SPCS ingress endpoint.
- The target Snowflake-managed Iceberg database/schema exists, and `RUSTICE_HORIZON_ROLE` has access to the tables you want Rustice to read or write.

Check the Snowflake profile before deploying:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SELECT CURRENT_ORGANIZATION_NAME(), CURRENT_ACCOUNT_NAME(), CURRENT_REGION(), CURRENT_ROLE()"
```

## Deploy With Script

Use this path for the normal first deployment. It creates the Snowflake-side SPCS objects, pushes the selected Rustice image into the Snowflake image registry, waits for the service to become `READY`, and writes an `embucket-snow` config.

By default the script does not build an image locally. It pulls
`embucket/rustice:<RUSTICE_IMAGE_TAG>` from Docker Hub, retags it, and pushes it
into the Snowflake image registry required by SPCS. Use `RUSTICE_BUILD_LOCAL=1`
only for development or PR validation from a local checkout.

Deploy the default one-node `CPU_X64_XS` service:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_SPCS \
RUSTICE_HORIZON_ROLE=<role-with-iceberg-access> \
RUSTICE_GRANT_TO_ROLE=<role-used-by-snowflake-profile> \
RUSTICE_CLIENT_DATABASE=rustice_spcs \
RUSTICE_CLIENT_SCHEMA=public \
RUSTICE_IMAGE_TAG=latest \
./deploy/spcs/deploy.sh
```

Use the role returned by `CURRENT_ROLE()` as `RUSTICE_GRANT_TO_ROLE` when that same profile should also run `embucket-snow` through the generated config.

For local PR testing, use the same command with `RUSTICE_BUILD_LOCAL=1` and a unique tag:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_SPCS \
RUSTICE_HORIZON_ROLE=<role-with-iceberg-access> \
RUSTICE_GRANT_TO_ROLE=<role-used-by-snowflake-profile> \
RUSTICE_CLIENT_DATABASE=rustice_spcs \
RUSTICE_CLIENT_SCHEMA=public \
RUSTICE_BUILD_LOCAL=1 \
RUSTICE_IMAGE_TAG=pr-test \
./deploy/spcs/deploy.sh
```

The script waits until the service is `READY`, prints the public ingress URL, and writes:

- `deploy/spcs/generated/config.toml`
- `deploy/spcs/generated/embucket_spcs_token` as a fallback token file
- `deploy/spcs/generated/embucket_spcs.env`

The generated `config.toml` uses `spcs_token_connection = "<SNOW_CONNECTION>"`, so the normal `embucket-snow` path gets short-lived SPCS ingress tokens in memory from the regular Snowflake profile. No daily token-file refresh is needed for that path.

For the current manual Snowflake-managed Iceberg read compatibility matrix, see
[ICEBERG_COMPATIBILITY.md](ICEBERG_COMPATIBILITY.md).

## Deploy With SQL

Use [deploy.sql](deploy.sql) when you want a worksheet-friendly deployment without running the shell script. SQL can create the Snowflake resources, but it cannot pull, tag, or push Docker images and cannot write local `embucket-snow` config files.

First create the image repository, then push the image into Snowflake's registry:

```sql
CREATE DATABASE IF NOT EXISTS RUSTICE_APP;
CREATE SCHEMA IF NOT EXISTS RUSTICE_APP.PUBLIC;
CREATE IMAGE REPOSITORY IF NOT EXISTS RUSTICE_APP.PUBLIC.RUSTICE_REPO;

SELECT LOWER(REPLACE(CURRENT_ORGANIZATION_NAME() || '-' || CURRENT_ACCOUNT_NAME(), '_', '-'))
  || '.registry.snowflakecomputing.com' AS registry_host;
```

```bash
snow --config-file /path/to/config.toml spcs image-registry login -c snowflake

docker pull --platform linux/amd64 embucket/rustice:latest
docker tag embucket/rustice:latest \
  <registry_host>/rustice_app/public/rustice_repo/rustice:latest
docker push \
  <registry_host>/rustice_app/public/rustice_repo/rustice:latest
```

Then edit section `0. Parameters` in [deploy.sql](deploy.sql), especially:

- `RUSTICE_HORIZON_DATABASE`
- `RUSTICE_HORIZON_ROLE`
- `RUSTICE_HORIZON_TABLES` when existing tables should be visible without eager listing
- `RUSTICE_IMAGE_TAG` when not using `latest`

Run the SQL file:

```bash
snow --config-file /path/to/config.toml \
  sql -c snowflake \
  --filename deploy/spcs/deploy.sql
```

After SQL deployment, get the ingress host:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW ENDPOINTS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE"
```

Grant the service endpoint to the role used by the regular Snowflake CLI profile if you want `embucket-snow` to fetch short-lived ingress tokens in memory:

```sql
GRANT USAGE ON DATABASE RUSTICE_APP TO ROLE <role-used-by-snowflake-profile>;
GRANT USAGE ON SCHEMA RUSTICE_APP.PUBLIC TO ROLE <role-used-by-snowflake-profile>;
GRANT SERVICE ROLE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE!RUSTICE_USER TO ROLE <role-used-by-snowflake-profile>;
```

Create an `embucket-snow` config manually:

```toml
default_connection_name = "embucket_spcs"

[connections.embucket_spcs]
host = "<ingress_url from SHOW ENDPOINTS>"
protocol = "https"
port = 443
account = "embucket"
user = "embucket"
password = "embucket"
database = "rustice_spcs"
schema = "public"
warehouse = "embucket"
spcs_token_connection = "snowflake"
spcs_token_config_file = "/path/to/config.toml"
```

`spcs_token_connection` points at the regular Snowflake CLI profile that can access the service endpoint. `embucket-snow` uses that profile to issue short-lived SPCS ingress tokens in memory.

The `account`, `user`, `password`, `database`, `schema`, and `warehouse` fields are compatibility values for the Snowflake-compatible client surface. In trusted SPCS mode, ingress authentication comes from `spcs_token_connection`, not from the placeholder password.

As a fallback, [deploy.sql](deploy.sql) also returns an ingress `token_secret` once. Store it next to the config as `embucket_spcs_token` with local user-only permissions:

```bash
umask 077
printf '%s' '<token_secret>' > embucket_spcs_token
```

## Verify With Snowflake CLI

These checks use regular Snowflake SQL, not Rustice. They verify that the SPCS infrastructure is up and that the baseline Iceberg table is visible to Snowflake:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SELECT SYSTEM\$GET_SERVICE_STATUS('RUSTICE_APP.PUBLIC.RUSTICE_SERVICE')"

snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW ENDPOINTS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE"

snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW SERVICE CONTAINERS IN SERVICE RUSTICE_APP.PUBLIC.RUSTICE_SERVICE"
```

Expected service state:

- `SYSTEM$GET_SERVICE_STATUS` contains `"status":"READY"`.
- `SHOW ENDPOINTS` returns an `ingress_url` ending with `.snowflakecomputing.app`.
- `SHOW SERVICE CONTAINERS` shows the image tag you deployed and `instance_status = READY`.

## Verify With Embucket CLI

These checks go through the Rustice Snowflake-compatible REST API running inside SPCS:

```bash
embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "CREATE TABLE IF NOT EXISTS rustice_spcs.public.smoke (id INT, msg STRING)"

embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "INSERT INTO rustice_spcs.public.smoke VALUES (1, 'ok')"

embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "SELECT * FROM rustice_spcs.public.smoke ORDER BY id"
```

Expected result for the standard smoke table:

```text
+----------+
| id | msg |
|----+-----|
| 1  | ok  |
+----------+
```

Optional write/create smoke:

```bash
embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "CREATE OR REPLACE TABLE rustice_spcs.public.rustice_write_smoke (id INT, msg STRING)"

embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "INSERT INTO rustice_spcs.public.rustice_write_smoke VALUES (1, 'written through spcs')"

embucket-snow --config-file deploy/spcs/generated/config.toml \
  sql -c embucket_spcs \
  -q "SELECT * FROM rustice_spcs.public.rustice_write_smoke ORDER BY id"
```

Verify the same table from regular Snowflake SQL. Current Rustice REST create behavior preserves lower-case table names, so quote the identifier:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SHOW ICEBERG TABLES LIKE 'rustice_write_smoke' IN SCHEMA RUSTICE_SPCS.PUBLIC"

snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SELECT * FROM RUSTICE_SPCS.PUBLIC.\"rustice_write_smoke\" ORDER BY 1"
```

## Image

`rustice` already has:

- A root `Dockerfile` that builds `embucketd`.
- A release workflow that publishes Docker Hub image `embucket/rustice`.

The deploy script uses `embucket/rustice:latest` by default. The normal user path is to use that Docker Hub image and let the script copy it into Snowflake's image registry. Set `RUSTICE_BUILD_LOCAL=1` only while testing changes from a local checkout.

SPCS currently requires `linux/amd64` images, so the script uses that platform when it pulls or builds the image.

The default mode is `RUSTICE_HORIZON_AUTH=pat`:

1. Creates a `TYPE = SERVICE` user.
2. Grants `RUSTICE_HORIZON_ROLE` to that user.
3. Generates a role-restricted programmatic access token (PAT).
4. Stores the PAT in a Snowflake `SECRET`.
5. Mounts the secret into the SPCS container as `ICEBERG_REST_CREDENTIAL`.

`rustice` exchanges that credential for a Horizon Catalog access token at startup and uses `ICEBERG_REST_PREFIX` as the Horizon database/prefix.
The SQL catalog name exposed by Rustice is configured separately through `RUSTICE_CLIENT_DATABASE` and is passed to the container as `ICEBERG_REST_CATALOG`. For example, `RUSTICE_HORIZON_DATABASE=RUSTICE_SPCS` with `RUSTICE_CLIENT_DATABASE=rustice_spcs` lets users query `rustice_spcs.public.<table>` while Horizon stores the tables under Snowflake database `RUSTICE_SPCS`.

## Dry Run SQL

Generate SQL with `RUSTICE_DRY_RUN=1` when you want to review or adapt the exact DDL produced by the shell script before running it in Snowsight or through `snow sql`:

```bash
RUSTICE_DRY_RUN=1 \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_SPCS \
RUSTICE_HORIZON_ROLE=RUSTICE_SPCS_ROLE \
RUSTICE_HORIZON_TABLES=PUBLIC.SMOKE \
./deploy/spcs/deploy.sh > rustice-spcs.sql
```

The image must still exist in the Snowflake image repository before the service can start. Use `RUSTICE_SKIP_IMAGE_PUSH=1` only after the image has already been pushed.

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
RUSTICE_CLIENT_DATABASE=rustice_spcs
RUSTICE_CLIENT_SCHEMA=public
RUSTICE_S3_REGION=<optional-aws-region-for-copy-into-s3-sources>
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

The deploy script also creates an ingress-only service user/PAT by default with `RUSTICE_CREATE_INGRESS_PAT=1`. That PAT is written to `deploy/spcs/generated/embucket_spcs_token` with local user-only permissions as a fallback for non-interactive environments. The normal `embucket-snow` path does not need that file: it uses the regular Snowflake CLI profile named by `RUSTICE_CLIENT_TOKEN_CONNECTION` to issue a short-lived SPCS ingress token in memory for each CLI process. The role used by that Snowflake profile must be granted the SPCS service role; set `RUSTICE_GRANT_TO_ROLE=<profile-role>` during deploy to make the generated config work without a separate token file.

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

`SNOWFLAKE_ISSUER_HOST` is passed into the container automatically from the active account locator and region, for example `aa06228.us-east-2.aws.snowflakecomputing.com`. Rustice uses it to structurally validate the SPCS caller token `iss` claim. Override `RUSTICE_SNOWFLAKE_ISSUER_HOST` only when the account uses a non-standard issuer host.

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

Rustice redacts sensitive request and response headers in tracing output, including `Authorization`, cookies, and Snowflake caller token headers such as `Sf-Context-Current-User-Token`.

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
  -q "SELECT * FROM rustice_spcs.public.smoke"
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
SHOW ICEBERG TABLES IN DATABASE RUSTICE_SPCS;
SHOW ICEBERG TABLES IN SCHEMA RUSTICE_SPCS.PUBLIC;
DESCRIBE ICEBERG TABLE RUSTICE_SPCS.PUBLIC.SMOKE;
```

To run the deployment from Snowsight instead of Snowflake CLI, first push the image into the Snowflake image repository once. Then either run [deploy.sql](deploy.sql) in a worksheet or run the script with `RUSTICE_SKIP_IMAGE_PUSH=1 RUSTICE_DRY_RUN=1` and paste the emitted SQL into a worksheet.

## End-to-End Smoke Test

First create a Snowflake-managed Iceberg table in the Horizon database that Rustice will use as `ICEBERG_REST_PREFIX`:

```sql
CREATE DATABASE IF NOT EXISTS RUSTICE_SPCS;
CREATE SCHEMA IF NOT EXISTS RUSTICE_SPCS.PUBLIC;
ALTER SCHEMA RUSTICE_SPCS.PUBLIC SET EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';
ALTER SCHEMA RUSTICE_SPCS.PUBLIC SET CATALOG = 'SNOWFLAKE';

CREATE OR REPLACE ICEBERG TABLE RUSTICE_SPCS.PUBLIC.SMOKE (
  ID INT,
  MSG STRING
)
  CATALOG = 'SNOWFLAKE'
  EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';

INSERT INTO RUSTICE_SPCS.PUBLIC.SMOKE VALUES (1, 'ok');

CREATE ROLE IF NOT EXISTS RUSTICE_SPCS_ROLE;
GRANT USAGE ON DATABASE RUSTICE_SPCS TO ROLE RUSTICE_SPCS_ROLE;
GRANT MONITOR ON DATABASE RUSTICE_SPCS TO ROLE RUSTICE_SPCS_ROLE;
GRANT USAGE ON SCHEMA RUSTICE_SPCS.PUBLIC TO ROLE RUSTICE_SPCS_ROLE;
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON TABLE RUSTICE_SPCS.PUBLIC.SMOKE TO ROLE RUSTICE_SPCS_ROLE;
GRANT CREATE TABLE ON SCHEMA RUSTICE_SPCS.PUBLIC TO ROLE RUSTICE_SPCS_ROLE;
GRANT CREATE ICEBERG TABLE ON SCHEMA RUSTICE_SPCS.PUBLIC TO ROLE RUSTICE_SPCS_ROLE;
```

For Horizon write/create checks, Snowflake also requires the write path to be enabled for the account and the role to satisfy Horizon write privileges. In particular, creating an Iceberg table through Horizon requires `CREATE ICEBERG TABLE` on the schema and `USAGE` on the external volume used by the table. Grant external-volume access with your account-specific volume name when applicable:

```sql
GRANT USAGE ON EXTERNAL VOLUME <external_volume_name> TO ROLE RUSTICE_SPCS_ROLE;
```

Deploy Rustice with:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_SPCS \
RUSTICE_HORIZON_ROLE=RUSTICE_SPCS_ROLE \
RUSTICE_INSTANCE_FAMILY=CPU_X64_XS \
RUSTICE_POOL_MIN_NODES=1 \
RUSTICE_POOL_MAX_NODES=1 \
RUSTICE_MIN_INSTANCES=1 \
RUSTICE_MAX_INSTANCES=1 \
RUSTICE_AUTO_SUSPEND_SECS=0 \
RUSTICE_HORIZON_SCHEMAS=PUBLIC,public \
RUSTICE_HORIZON_TABLES=PUBLIC.SMOKE \
RUSTICE_CLIENT_DATABASE=rustice_spcs \
RUSTICE_CLIENT_SCHEMA=public \
./deploy/spcs/deploy.sh
```

If your Horizon/object-store host is not covered by the automatically generated EAI allowlist, rerun with an explicit override:

```bash
RUSTICE_EGRESS_HOSTS=<catalog-host>,<object-store-host> ./deploy/spcs/deploy.sh
```

Verify the baseline Snowflake-managed Iceberg table through regular Snowflake SQL. This query uses Snowflake compute and validates that the source table exists and is readable by the active Snowflake role:

```bash
snow --config-file /path/to/config.toml sql -c snowflake \
  -q "SELECT * FROM RUSTICE_SPCS.PUBLIC.SMOKE"
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

After the service reaches `READY`, run this SQL through the Embucket/Rustice Snowflake-compatible endpoint with the patched client/connector. `rustice_spcs` is the Rustice SQL catalog configured by `RUSTICE_CLIENT_DATABASE`; `RUSTICE_SPCS` is the underlying Horizon/Snowflake database configured by `RUSTICE_HORIZON_DATABASE`:

```sql
SHOW DATABASES;
SHOW SCHEMAS IN DATABASE rustice_spcs;
SHOW TABLES IN SCHEMA rustice_spcs.public;
SELECT * FROM rustice_spcs.public.smoke;
```

If Horizon write access is enabled and the role has the required write privileges, use a separate write smoke:

```sql
CREATE TABLE rustice_spcs.public.rustice_write_smoke (
  id INT,
  msg STRING
);

INSERT INTO rustice_spcs.public.rustice_write_smoke VALUES (2, 'written by rustice');
SELECT * FROM rustice_spcs.public.rustice_write_smoke;
DROP TABLE rustice_spcs.public.rustice_write_smoke;
```

To inspect a table created through Rustice from regular Snowflake SQL or Snowsight, query the underlying Horizon database and schema. Current Rustice REST create behavior preserves the lower-case table name, so quote the table identifier in Snowflake SQL:

```sql
SHOW ICEBERG TABLES LIKE 'rustice_write_smoke' IN SCHEMA RUSTICE_SPCS.PUBLIC;
SELECT * FROM RUSTICE_SPCS.PUBLIC."rustice_write_smoke";
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
