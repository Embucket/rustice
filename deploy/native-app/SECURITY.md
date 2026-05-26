# Native App Security Evidence

This document maps Rustice Native App controls to the Snowflake Native Apps with
Snowpark Container Services security review questionnaire.

## Application Overview

Application name: Embucket Native App / Rustice Native App.

The app runs a Rust/DataFusion SQL endpoint inside Snowpark Container Services
in the consumer account. It exposes a Snowflake-compatible REST endpoint through
SPCS public ingress and queries Snowflake-managed Iceberg tables through
Snowflake Horizon / REST Catalog.

Primary use cases:

- cost-optimized SQL compute for Snowflake-managed Iceberg tables;
- dbt-core and dbt Snowplow workloads;
- batch SQL transformations running in the consumer Snowflake account;
- private Native App distribution through Snowflake Marketplace listings.

## Architecture Documentation

Architecture overview and data-flow diagram:

- `deploy/native-app/README.md#architecture`

Core package files:

- `deploy/native-app/app/manifest.yml`
- `deploy/native-app/app/setup_script.sql`
- `deploy/native-app/service/rustice_spec.yaml`
- `Dockerfile`

## Components

Containers:

- One container named `rustice` running the `embucketd` Rust binary.
- The image is built from a multi-stage Dockerfile.
- The runtime stage uses `gcr.io/distroless/cc-debian12`.
- The runtime container runs as `nonroot:nonroot`.

Public endpoints:

- One SPCS public ingress endpoint named `main` on port `3000`.
- The endpoint serves the Snowflake-compatible REST API and `/health` readiness
  endpoint.
- Public access is protected by Snowflake SPCS ingress authentication.

External integrations:

- No provider-hosted external services are called by the app.
- The app creates a consumer-approved External Access Integration for the
  consumer Snowflake account host, the configured object-store host, and optional
  consumer-provided extra hosts.
- The app reads a consumer-approved Snowflake `GENERIC_STRING` secret reference
  named `horizon_credential_secret`.

UDFs:

- None. The app creates SQL stored procedures, but no UDFs.

Machine learning models:

- None.

Runtime code downloads:

- None. The runtime image contains the application binary and does not download
  additional application code.

## Authentication and Authorization

- Consumers install the app through Snowflake Native Apps.
- App operations are exposed through the `app_user` application role.
- The service runs with `executeAsCaller: true`.
- The public endpoint is protected by Snowflake SPCS ingress.
- The app does not create or own Horizon credentials. Consumers bind a Snowflake
  `SECRET` reference with `READ` privilege.
- Egress is constrained by Snowflake External Access Integration network rules.

## Data Access and Storage

Consumer data accessed by the app:

- SQL query text sent by authenticated callers;
- query results returned to authenticated callers;
- Iceberg catalog metadata and manifests for configured tables;
- object-store files for configured Snowflake-managed Iceberg tables;
- the consumer-approved Horizon credential secret mounted by Snowflake.

Consumer data stored outside the consumer account:

- None. The app does not store consumer data, credentials, keys, models, or logs
  outside the consumer account.

Provider data accessed at runtime:

- None, except for artifacts included in or referenced by the Native App package
  and the Snowflake-hosted container image repository used by the package.

Operational logs:

- Logs are written to Snowflake Native App / SPCS service logs in the consumer
  account.

## Objects and Privileges

Requested account privileges:

- `CREATE COMPUTE POOL`
- `BIND SERVICE ENDPOINT`
- `CREATE EXTERNAL ACCESS INTEGRATION`

Requested references:

- `READ` on the consumer-provided `SECRET` reference
  `horizon_credential_secret`.

Objects created by the app:

- application role `app_user`;
- schemas `core` and `app_public`;
- table `core.rustice_config`;
- secret `core.rustice_jwt_secret`;
- SQL procedures for reference registration, external access configuration,
  service lifecycle, service status, endpoints, and logs;
- network rule `core.rustice_egress_rule`;
- external access integration `<app>_RUSTICE_EAI`;
- compute pool `<app>_RUSTICE_POOL`;
- service `core.rustice_service`;
- service role `core.rustice_service!rustice_user`;
- public SPCS endpoint `main` on port `3000`.

Unauthenticated functionality:

- None. Functionality is accessed through Snowflake Native App roles and
  Snowflake SPCS ingress.

## Security Assurance

The repository-level security policy is documented in `SECURITY.md`.

For Native App publication or Snowflake review, maintainers should keep evidence
for:

- pull request review and CI results;
- `cargo fmt`, `cargo clippy`, and Rust tests;
- final image vulnerability scan report;
- final image malware scan report;
- any vulnerability exceptions or false-positive rationale.

## Image Scan Procedure

The GitHub Actions workflow `.github/workflows/image-security.yml` builds the
final image and scans it with:

- Grype for CVEs, uploading the full vulnerability report;
- a release gate for fixable `HIGH` and `CRITICAL` findings;
- ClamAV for malware, scanning the saved final image archive.

Run it manually before Snowflake questionnaire resubmission:

```text
GitHub Actions -> image-security-scan -> Run workflow
```

The workflow uploads scan artifacts that can be attached to or linked from the
Snowflake security questionnaire.
