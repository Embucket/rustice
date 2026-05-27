# api-snowflake-rest agent guide

## What this crate owns
- Snowflake V1-compatible REST routes, request/response models, and Axum handlers.
- Login/query/abort/session/heartbeat flow that authenticates, resolves a session, calls `ExecutionService`, and serializes results.
- JSON and Arrow response formatting, Snowflake REST error envelopes, SQL state selection, retry-by-request-ID behavior, and REST integration test helpers.
- REST config for demo credentials, JWT secret, trusted SPCS ingress, and result serialization format.

## What this crate must not own
- SQL execution, planning, UDF behavior, catalog mutations, or metadata storage.
- JWT validation/extraction internals and session cookie propagation; those belong in `api-snowflake-rest-sessions`.
- Daemon listener/tracing setup; that belongs in `embucketd`.
- Snowflake passthrough/fallback except as an explicit routing contract at the REST boundary.

## Important files and modules
- `src/models.rs` defines Snowflake REST request/response payloads and row metadata serialization.
- `src/server/router.rs` wires public and authenticated Axum routes plus compression/decompression.
- `src/server/handlers.rs` contains endpoint extractors and thin handler functions.
- `src/server/logic.rs` implements login, query context construction, retry handling, and execution calls.
- `src/server/helpers.rs` converts `QueryResult` into JSON rowsets or base64 Arrow IPC.
- `src/server/error.rs` maps internal errors to HTTP status, `SqlState`, and Snowflake error payloads.
- `src/server/core_state.rs`, `state.rs`, and `server_models.rs` assemble executor state and REST config.
- `src/tests/` provides an offline local HTTP server, `snow_sql` helper, SQL snapshot macro, gzip tests, and response snapshots.

## Local verification
- `cargo test -p api-snowflake-rest`
- Protocol snapshots: `cargo test -p api-snowflake-rest -- test_rest_api`
- Compression behavior: `cargo test -p api-snowflake-rest -- test_gzip_encoding`
- Retry-contract changes: `cargo test -p api-snowflake-rest --features retry-disable`

## Common failure modes
- Changing JSON field names, casing, rowset shape, or Arrow base64 behavior without updating snapshots.
- Returning the wrong HTTP status, `sqlState`, or Snowflake error code for executor errors.
- Treating `asyncExec` as implemented; `logic.rs` currently returns `NotImplemented`.
- Mixing session extraction/JWT logic into handlers instead of using `TokenizedSession`.
- Breaking offline tests by requiring an external Snowflake service or networked catalog.
