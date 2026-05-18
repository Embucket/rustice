# Deploy Rustice to Snowpark Container Services

This directory deploys the `embucketd` container from `rustice` to Snowpark Container Services (SPCS) by using Snowflake CLI SQL commands.

## Image

`rustice` already has:

- A root `Dockerfile` that builds `embucketd`.
- A release workflow that publishes Docker Hub image `embucket/rustice`.

The deploy script uses `embucket/rustice:latest` by default. Set `RUSTICE_BUILD_LOCAL=1` to build the local checkout instead.

SPCS currently requires `linux/amd64` images, so the script uses that platform when it pulls or builds the image.

## Quick Start

Run with a Snowflake CLI connection that can create SPCS resources, external access integrations, service users, and PATs:

```bash
SNOW_CONFIG_FILE=/path/to/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=ANALYTICS \
RUSTICE_HORIZON_ROLE=DATA_ENGINEER \
./deploy/spcs/deploy.sh
```

The default mode is `RUSTICE_HORIZON_AUTH=pat`:

1. Creates a `TYPE = SERVICE` user.
2. Grants `RUSTICE_HORIZON_ROLE` to that user.
3. Generates a role-restricted programmatic access token (PAT).
4. Stores the PAT in a Snowflake `SECRET`.
5. Mounts the secret into the SPCS container as `ICEBERG_REST_CREDENTIAL`.

`rustice` exchanges that credential for a Horizon Catalog access token at startup and uses `ICEBERG_REST_PREFIX` as the Horizon database/prefix.
The SQL catalog name exposed by Rustice remains `embucket`; the Horizon database/prefix is configured separately through `RUSTICE_HORIZON_DATABASE`.

## Common Options

```bash
RUSTICE_DB=RUSTICE_APP
RUSTICE_SCHEMA=PUBLIC
RUSTICE_COMPUTE_POOL=RUSTICE_POOL
RUSTICE_INSTANCE_FAMILY=CPU_X64_XS
RUSTICE_IMAGE_TAG=latest
RUSTICE_REGISTRY_LOGIN=1
RUSTICE_GRANT_TO_ROLE=ANALYST
RUSTICE_AUTO_SUSPEND_SECS=300
RUSTICE_BUILD_LOCAL=1
RUSTICE_DRY_RUN=1
```

The script uses `snow spcs image-registry login` by default before pushing the image. Docker or Podman must be available and running locally; in WSL, Docker Desktop WSL integration must be enabled. Set `RUSTICE_REGISTRY_LOGIN=0` only if the container CLI is already logged in to the Snowflake registry.

Use `RUSTICE_HORIZON_AUTH=none` to deploy only the service shell without Horizon credentials.

Use `RUSTICE_HORIZON_AUTH=bearer_token` or `oauth_token` if you already manage a Snowflake `SECRET` containing a token. In that case set `RUSTICE_HORIZON_SECRET=<db>.<schema>.<secret>`.

If Horizon calls fail because Snowflake redirects to another account hostname, set `RUSTICE_EGRESS_HOSTS` to a comma-separated allowlist, for example `<org>-<account>.snowflakecomputing.com,<locator>.<region>.<cloud>.snowflakecomputing.com`.

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

## Minimal Iceberg Smoke Test

First create a Snowflake-managed Iceberg table in the Horizon database that Rustice will use as `ICEBERG_REST_PREFIX`:

```sql
CREATE DATABASE IF NOT EXISTS RUSTICE_E2E;
CREATE SCHEMA IF NOT EXISTS RUSTICE_E2E.PUBLIC;

CREATE OR REPLACE ICEBERG TABLE RUSTICE_E2E.PUBLIC.SMOKE (
  ID INT,
  MSG STRING
)
  CATALOG = 'SNOWFLAKE'
  EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';

INSERT INTO RUSTICE_E2E.PUBLIC.SMOKE VALUES (1, 'ok');

CREATE ROLE IF NOT EXISTS RUSTICE_E2E_ROLE;
GRANT USAGE ON DATABASE RUSTICE_E2E TO ROLE RUSTICE_E2E_ROLE;
GRANT USAGE ON SCHEMA RUSTICE_E2E.PUBLIC TO ROLE RUSTICE_E2E_ROLE;
GRANT SELECT, INSERT ON TABLE RUSTICE_E2E.PUBLIC.SMOKE TO ROLE RUSTICE_E2E_ROLE;
GRANT CREATE TABLE ON SCHEMA RUSTICE_E2E.PUBLIC TO ROLE RUSTICE_E2E_ROLE;
GRANT CREATE ICEBERG TABLE ON SCHEMA RUSTICE_E2E.PUBLIC TO ROLE RUSTICE_E2E_ROLE;
```

Deploy Rustice with:

```bash
SNOW_CONFIG_FILE=/home/artem/.snowflake/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_E2E \
RUSTICE_HORIZON_ROLE=RUSTICE_E2E_ROLE \
RUSTICE_INSTANCE_FAMILY=CPU_X64_XS \
RUSTICE_POOL_MIN_NODES=1 \
RUSTICE_POOL_MAX_NODES=1 \
RUSTICE_MIN_INSTANCES=1 \
RUSTICE_MAX_INSTANCES=1 \
RUSTICE_AUTO_SUSPEND_SECS=300 \
./deploy/spcs/deploy.sh
```

After the service reaches `READY`, run these SQL statements through the Rustice/Snowflake-compatible endpoint. The SQL catalog name remains `embucket`; `RUSTICE_E2E` is the underlying Horizon prefix:

```sql
SELECT * FROM embucket.public.smoke;

CREATE TABLE embucket.public.rustice_write_smoke (
  id INT,
  msg STRING
);

INSERT INTO embucket.public.rustice_write_smoke VALUES (2, 'written by rustice');
SELECT * FROM embucket.public.rustice_write_smoke;
DROP TABLE embucket.public.rustice_write_smoke;
```

Snowflake SQL can manage and inspect the SPCS service directly, but it does not speak the Snowflake REST session protocol that Rustice exposes. For SQL execution against Rustice itself, use the modified Snowflake-compatible client or connector pointed at the SPCS ingress endpoint.

## Security Notes

- No local RSA keypair is required for the default path.
- The PAT is role-restricted and stored as a Snowflake `SECRET`; it is not written to the repo.
- The SPCS service is created with `executeAsCaller: true`, so Snowflake ingress passes caller context headers to the container.
- Horizon Catalog calls are still made with the service user's role. Grant that role only the Iceberg table privileges this service is allowed to exercise.

## Current Limitation

The current Rustice Horizon integration performs credential exchange at service startup. For long-running production services, add token refresh or restart/rotate the service before the exchanged Horizon token expires.

References:

- SPCS overview: https://docs.snowflake.com/en/developer-guide/snowpark-container-services/overview
- SPCS service specification: https://docs.snowflake.com/en/developer-guide/snowpark-container-services/specification-reference
- Horizon Catalog for Iceberg tables: https://docs.snowflake.com/en/user-guide/tables-iceberg-access-using-external-query-engine-snowflake-horizon
- Programmatic access tokens: https://docs.snowflake.com/en/user-guide/programmatic-access-tokens
