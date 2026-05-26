# api-snowflake-rest-sessions agent guide

## What this crate owns
- Snowflake REST session identity: `TokenizedSession`, JWT claim creation/validation, session cookies, and SPCS ingress-derived sessions.
- Axum extractors and middleware helpers that attach a session ID to authenticated requests.
- Session TTL cleanup through `SessionStore` using the executor's `ExecutionService`.
- Redaction of sensitive auth/session headers for tracing.

## What this crate must not own
- REST endpoint request/response bodies or SQL API semantics; those belong in `api-snowflake-rest`.
- Query execution or running-query state beyond calling the `ExecutionService` trait.
- Catalog, metastore, or function behavior.
- Broad auth-provider policy beyond the visible JWT, cookie, and trusted SPCS ingress mechanics.

## Important files and modules
- `src/session.rs` defines `TokenizedSession`, `SessionStore`, auth-token extraction, SPCS ingress header handling, cookie parsing, and tests.
- `src/helpers.rs` creates and validates JWT claims with audience and expiry checks.
- `src/layer.rs` implements `Host` extraction and session-cookie propagation.
- `src/error.rs` maps extraction/auth/session failures into Axum responses.
- `src/lib.rs` exposes the session modules and `TokenizedSession`.

## Local verification
- `cargo test -p api-snowflake-rest-sessions`
- Focused auth/session tests: `cargo test -p api-snowflake-rest-sessions -- session`
- End-to-end REST auth checks live in `api-snowflake-rest`: `cargo test -p api-snowflake-rest -- test_rest_api`

## Common failure modes
- Validating JWTs without the request `Host` audience, or changing leeway/expiry without checking login flow.
- Logging raw `authorization`, cookie, or `*-token` header values.
- Trusting SPCS ingress headers when the app state has not enabled trusted ingress.
- Assuming cookie-derived sessions include login metadata; cookies carry only the session ID.
- Changing the 4-hour session TTL without checking daemon cleanup and session tests.
