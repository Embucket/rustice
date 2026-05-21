# Rustice Native App with SPCS

This directory is the first Snowflake Native App packaging scaffold for running
the Rustice container as a Snowpark Container Services service in a consumer
account.

The existing `deploy/spcs` scripts are still the fastest way to deploy directly
from a repository checkout. This Native App package is the Marketplace/private
listing path: the provider publishes an application package, and the consumer
installs and activates it from Snowsight.

## Current Scope

This scaffold packages the same `embucketd` container service that
`deploy/spcs/deploy.sh` creates:

- one `CPU_X64_XS` compute pool by default
- public ingress endpoint on port `3000`
- `executeAsCaller: true`
- Rustice trusted SPCS ingress mode
- Horizon/Snowflake REST Catalog settings passed as service environment
- SPCS service OAuth token mounted from `/snowflake/session/token`

The Horizon auth mode is intentionally experimental in this scaffold. The
direct SPCS deploy script uses a service-user PAT stored in a Snowflake secret.
That pattern works in our account, but it is not yet a good Marketplace UX
because consumers would still need to manage a PAT. The Native App service spec
therefore uses `ICEBERG_REST_OAUTH_TOKEN_FILE=/snowflake/session/token` so we
can test whether Snowflake-managed service credentials are accepted by Horizon
from inside an app. If they are not, the next implementation step is a
consumer-approved secret/reference flow for `ICEBERG_REST_CREDENTIAL`.

## Provider Setup

Create a provider-side image repository outside the application package and
push the Rustice image there. Native Apps with containers cannot reference
Docker Hub directly.

```bash
snow --config-file /path/to/.snowflake/config.toml sql -c snowflake -q "
CREATE DATABASE IF NOT EXISTS RUSTICE_NATIVE_APP_IMAGES;
CREATE SCHEMA IF NOT EXISTS RUSTICE_NATIVE_APP_IMAGES.PUBLIC;
CREATE IMAGE REPOSITORY IF NOT EXISTS RUSTICE_NATIVE_APP_IMAGES.PUBLIC.RUSTICE_REPO;
"

snow --config-file /path/to/.snowflake/config.toml spcs image-registry login -c snowflake

docker build --platform linux/amd64 -t rustice-native-app:latest /path/to/rustice
docker tag rustice-native-app:latest \
  <org>-<account>.registry.snowflakecomputing.com/rustice_native_app_images/public/rustice_repo/rustice:latest
docker push \
  <org>-<account>.registry.snowflakecomputing.com/rustice_native_app_images/public/rustice_repo/rustice:latest
```

If you change the image repository database, schema, repository, image name, or
tag, update the image path in `app/manifest.yml` and the default image path in
`app/setup_script.sql`.

## Local App Test

From this directory:

```bash
snow app run --config-file /path/to/.snowflake/config.toml -c snowflake
```

After the development application is created, grant the account privileges that
SPCS requires:

```sql
GRANT CREATE COMPUTE POOL ON ACCOUNT TO APPLICATION RUSTICE_NATIVE_APP;
GRANT BIND SERVICE ENDPOINT ON ACCOUNT TO APPLICATION RUSTICE_NATIVE_APP;
```

Configure external access and create the service:

```sql
CALL RUSTICE_NATIVE_APP.APP_PUBLIC.CONFIGURE_EXTERNAL_ACCESS(
  'RUSTICE_SPCS',
  'RUSTICE_SPCS',
  'rustice_spcs',
  'public_snowplow_manifest',
  'public_snowplow_manifest,public_snowplow_manifest_derived,public_snowplow_manifest_scratch,public_snowplow_manifest_snowplow_manifest',
  '',
  'us-east-2',
  'embucket-testdata.s3.us-east-2.amazonaws.com'
);

-- Approve the generated external access app specification in Snowsight if the
-- app shows a pending external access request.

CALL RUSTICE_NATIVE_APP.APP_PUBLIC.START_APP();
CALL RUSTICE_NATIVE_APP.APP_PUBLIC.SERVICE_STATUS();
CALL RUSTICE_NATIVE_APP.APP_PUBLIC.SERVICE_ENDPOINTS();
```

The arguments are:

- `horizon_database`: Snowflake database backing the Horizon catalog prefix.
- `horizon_role`: role/scope to use for Horizon access. Kept for parity with
  the direct SPCS script; the current OAuth-file experiment does not exchange a
  PAT for this role.
- `client_database`: SQL catalog name exposed by Rustice.
- `client_schema`: default SQL schema exposed by Rustice.
- `horizon_schemas`: comma-separated schemas to bootstrap lazily.
- `horizon_tables`: comma-separated `schema.table` names to bootstrap lazily.
- `s3_region`: AWS region for `COPY INTO s3://...` sources.
- `extra_egress_hosts`: comma-separated hosts in addition to the Snowflake
  account host and regional S3 host.

## Consumer Flow for a Private Listing

After the app package is attached to a private listing, the consumer installs it
from `Catalog -> Apps`, grants requested privileges, approves external access,
and calls the same procedures:

```sql
CALL <installed_app>.APP_PUBLIC.CONFIGURE_EXTERNAL_ACCESS(...);
CALL <installed_app>.APP_PUBLIC.START_APP();
CALL <installed_app>.APP_PUBLIC.SERVICE_ENDPOINTS();
```

The consumer then points `embucket-snow` or dbt at the returned public ingress
host. For dbt Snowplow, use the runbook in
`../test-dbt-snowplow-web/README.md` with:

```bash
export EMBUCKET_SPCS=1
export EMBUCKET_HOST="<service>.snowflakecomputing.app"
export EMBUCKET_PORT=443
export EMBUCKET_PROTOCOL=https
export EMBUCKET_DATABASE=rustice_spcs
export EMBUCKET_SCHEMA=public_snowplow_manifest
export EMBUCKET_THREADS=1
```

## Publish a Private Listing

1. Build and push the provider image.
2. Test the application package with `snow app run`.
3. Create a version/patch for the application package.
4. In Provider Studio, create a listing for `Specified Consumers`.
5. Choose product type `Native App` and attach the application package.
6. Add consumer organization/account identifiers.
7. Publish the private listing.

Container Native Apps require Snowflake Product Security approval and automated
container image scanning before listings can be published.
