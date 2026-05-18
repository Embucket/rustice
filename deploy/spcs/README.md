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

The script uses `snow spcs image-registry login` by default before pushing the image. Docker or Podman must be available locally; set `RUSTICE_REGISTRY_LOGIN=0` only if the container CLI is already logged in to the Snowflake registry.

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

To inspect logs:

```sql
SELECT SYSTEM$GET_SERVICE_LOGS('RUSTICE_APP.PUBLIC.RUSTICE_SERVICE', 0, 'rustice', 100);
```

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
