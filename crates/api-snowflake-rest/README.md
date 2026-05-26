# api-snowflake-rest

Provides a REST API compatible with the Snowflake V1 SQL API, allowing Snowflake SDK clients and tooling to connect and interact with Embucket.

## Purpose

This crate allows tools and applications that use the Snowflake client SDKs to connect to Embucket as if it were a Snowflake instance, enabling query execution and other interactions via the Snowflake SQL API.

## Architecture

`make_snowflake_router(AppState) -> Router` builds the Axum router. Requests flow
`server/handlers.rs` → `server/logic.rs`, where they authenticate, resolve/refresh a session
(via `api-snowflake-rest-sessions`), call `ExecutionService` (from `executor`), and serialize
results in `server/helpers.rs`.

### Endpoints

- `POST /session/v1/login-request` — validate credentials, create session, return JWT.
- `POST /queries/v1/query-request` — execute SQL; supports retry by `request_id`.
- `POST /queries/v1/abort-request` — cancel a running query.
- `POST /session` (delete) and `POST /session/heartbeat`.

### Auth modes

Demo user/password (default `embucket`/`embucket`), JWT (HS256), or trusted SPCS ingress
headers when `trust_spcs_ingress` is enabled.

### Response shape

Snowflake V1: `data.rowtype[]` (column metadata), `data.rowset` (JSON) or `data.rowsetBase64`
(Arrow), `total` / `returned`, `queryResultFormat`, `sqlState` (`00000` on success),
`queryId`, and `errorCode` / `message` on failure (`sql_state.rs` defines the codes).

## `snow sql` programmatic API
`snow_sql` function provides a programmatic API to interact with the server
via Snowflake REST API in similar way as `snow sql` command line tool does.

There is also a `sql_test` macro which is a wrapper around `snow_sql` function. Test suite uses this helper macro to run SQLs like it was executed from `snow sql`.

## Testing
Run tests as usual:
```
cargo test --workspace
```
