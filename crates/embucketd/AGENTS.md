# embucketd agent guide

## What this crate owns
- The `embucketd` binary entry point and top-level server bootstrap.
- CLI/env parsing for daemon runtime settings, including catalog URL, auth demo credentials, tracing, memory/disk limits, and object-store timeouts.
- Construction of `RestApiConfig`, executor `Config`, `CoreState`, Axum routes, Swagger UI, health/telemetry endpoints, and global HTTP layers.
- Process-level tracing/OpenTelemetry setup, optional allocation tracing, listener binding, and graceful shutdown on signal or idle timeout.

## What this crate must not own
- SQL parsing, planning, execution, cancellation internals, or Snowflake error mapping; those belong in `executor`.
- Snowflake REST request/response semantics; those belong in `api-snowflake-rest`.
- JWT/session extraction details; those belong in `api-snowflake-rest-sessions`.
- Catalog/metastore data model or Iceberg table behavior.
- Snowflake passthrough/fallback behavior except wiring an explicit routing contract at the daemon boundary.

## Important files and modules
- `src/main.rs` builds the runtime, tracing provider, `CoreState`, Snowflake router, health routes, and shutdown task.
- `src/cli.rs` defines clap/env configuration. `--catalog-url` enters dev catalog mode only for `file:`, `s3:`, `http:`, or `https:` URLs.
- `src/helpers.rs` resolves bind addresses to IPv4 socket addresses.
- `src/layers.rs` is compiled only with `alloc-tracing` and aggregates allocation events per query/session.
- `README.md` lists the served routes and high-level daemon role.

## Local verification
- `cargo check -p embucketd`
- `cargo test -p embucketd`
- Local/offline smoke run with a file-backed dev catalog: `JWT_SECRET=secret cargo run -p embucketd -- --host 127.0.0.1 --port 3000 --catalog-url file:/tmp/rustice-catalog`

## Common failure modes
- Putting protocol or execution logic into `main.rs` instead of the owning crate.
- Forgetting that login requires a non-empty JWT secret unless trusted SPCS ingress is being used.
- Assuming every `CATALOG_URL` value selects dev catalog mode; the daemon matches only the URL prefixes in `main.rs`.
- Changing middleware or timeout placement without checking compressed/authenticated REST routes.
- Enabling allocation tracing paths without the `alloc-tracing` feature.
