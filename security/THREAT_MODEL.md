# Native App Threat Model

This threat model covers the Rustice Snowflake Native App with Snowpark
Container Services. The app runs a Rust/DataFusion SQL endpoint in the consumer
Snowflake account and accesses Snowflake-managed Iceberg tables through
Snowflake Horizon / REST Catalog.

## Scope

In scope:

- Snowflake Native App package objects under `deploy/native-app`;
- the `rustice` SPCS container running the `embucketd` binary;
- SPCS public ingress endpoint on port `3000`;
- Horizon / REST Catalog access from the container;
- Snowflake External Access Integration and network rule configuration;
- consumer-provided Snowflake `SECRET` reference for Horizon credentials;
- final runtime container image built from the repository `Dockerfile`.

Out of scope:

- Snowflake platform internals;
- consumer-managed Snowflake roles, tables, secrets, and account policies outside
  the app configuration;
- client-side orchestration tools except for their authenticated requests to the
  app endpoint;
- provider-hosted services, because the app does not call provider-hosted
  runtime services.

## Assets

- Consumer SQL query text and query results.
- Consumer Snowflake-managed Iceberg table metadata, manifests, and object-store
  files.
- Consumer-approved Horizon credential secret mounted through a Snowflake
  Native App reference.
- Snowflake SPCS ingress tokens and caller context.
- Native App package SQL, service specification, and runtime image.
- Operational logs stored in the consumer Snowflake account.

## Trust Boundaries

1. Consumer/client to SPCS ingress
   - Requests must pass Snowflake SPCS ingress authentication.
   - The app does not expose functionality intended for unauthenticated users.

2. SPCS ingress to Rustice container
   - The container trusts Snowflake-provided ingress/caller context only when
     running in trusted SPCS mode.
   - The service runs with `executeAsCaller: true`.

3. Rustice container to Horizon / REST Catalog
   - Egress is restricted by Snowflake External Access Integration network
     rules.
   - Horizon credentials are provided by a consumer-owned Snowflake `SECRET`
     reference and mounted at runtime.

4. Rustice container to object storage
   - Object access uses scoped credentials or access paths vended by the
     Snowflake/Horizon catalog flow.
   - Object-store hosts are constrained by EAI network rules.

5. Provider artifacts to consumer account
   - The provider publishes the Native App package and container image.
   - Runtime consumer data remains in the consumer Snowflake account.

## STRIDE Analysis

| Threat | Risk | Controls |
| --- | --- | --- |
| Spoofed public endpoint requests | Unauthenticated caller sends SQL to the service | SPCS public ingress authentication; Native App roles; no app functionality is intended to be reachable without Snowflake auth |
| Forged caller context headers outside SPCS | Caller identity could be spoofed if trusted ingress mode were exposed outside Snowflake | Trusted ingress mode is intended only for SPCS deployment; public endpoint is behind SPCS ingress; documentation warns not to expose trusted mode outside SPCS |
| Credential disclosure in image layers | Horizon or app secrets could leak through Docker image history | No credentials are baked into ARG/ENV or image layers; credentials are mounted through Snowflake secrets/references at runtime |
| Overbroad egress | Container could reach unauthorized external hosts | Egress uses Snowflake External Access Integration with explicit host network rules; no `0.0.0.0` egress |
| Unauthorized Horizon access | Service accesses tables beyond consumer intent | Consumer controls the Horizon role/PAT secret and grants; app uses a consumer-bound secret reference with `READ` privilege |
| Tampered runtime image | Malicious or vulnerable image is published | Authenticated GitHub source control; PR review; CI checks; final image Grype CVE scan; ClamAV malware scan; distroless nonroot runtime image |
| Dependency vulnerability | Reachable CVE in runtime dependencies | Grype scan gates High/Critical findings; patch SLAs in `SECURITY.md`; runtime image rebuild on remediation |
| Data exfiltration through provider systems | Consumer data leaves the consumer account | No provider-hosted runtime services; app does not store consumer data outside the consumer account; logs remain in Snowflake service logs |
| Secret misuse in logs | Credentials appear in service or CI logs | Secrets are mounted by Snowflake; sensitive SQL and secret values are redacted in deployment scripts; scan artifacts do not include secret values |
| Denial of service from expensive queries | Consumer compute pool resources are exhausted | Service runs in consumer-owned SPCS compute pool; consumers control pool size, lifecycle, and can stop/suspend the service |
| Metadata staleness or write conflicts | Mixed writers can read stale snapshots or hit optimistic commit conflicts | Horizon table metadata is loaded freshly for table resolution; true concurrent commit conflicts remain subject to Iceberg optimistic concurrency behavior |

## Security Assumptions

- Snowflake SPCS ingress correctly authenticates requests before forwarding them
  to the container.
- Consumers grant only the roles, secrets, and external access hosts required
  for their deployment.
- The Horizon credential secret is scoped to the intended role and account.
- Consumers operate the app inside Snowflake Native Apps / SPCS, not as an
  internet-exposed standalone container in trusted ingress mode.

## Residual Risks and Follow-Ups

- True concurrent Iceberg writers can still hit optimistic commit conflicts; the
  app should return clear retryable errors for those cases.
- Rustice's Snowflake-compatible API surface is still evolving and does not
  implement every Snowflake SQL behavior.
- Standalone Rustice `UPDATE` and `DELETE` on Iceberg tables are not part of the
  supported write surface until implemented and tested.
- Case-sensitive `MERGE` planning for some Snowflake-created uppercase schemas
  requires further hardening.

## Review Cadence

Maintainers review this threat model when:

- Native App package permissions change;
- the service specification, ingress mode, secret/reference flow, or egress
  behavior changes;
- the runtime image base changes;
- a material security finding or incident occurs;
- before resubmitting the app for Snowflake Native Apps with SPCS security
  review.
