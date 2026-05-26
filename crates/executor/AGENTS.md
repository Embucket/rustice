# executor agent guide

## What this crate owns
- Local SQL execution over DataFusion sessions and the Embucket catalog list.
- `ExecutionService`, `CoreExecutionService`, `UserSession`, `UserQuery`, query lifecycle, cancellation, timeouts, and running-query history.
- Snowflake-oriented parsing/post-processing, DDL/DML/SHOW/COPY/MERGE handling, custom DataFusion analyzers, optimizers, type planner, query planner, and merge physical plan.
- Mapping executor/catalog/function/DataFusion errors into Snowflake-shaped error codes and messages.

## What this crate must not own
- HTTP routing, JSON response envelopes, login, JWT, cookies, or SPCS ingress handling.
- Function implementation details beyond registering and invoking the `functions` crate.
- Catalog provider wrappers and metadata source-of-truth models, which belong in `catalog` and `catalog-metastore`.
- Snowflake passthrough/fallback except as an explicit routing/contract handoff; do not hide it in local execution paths.

## Important files and modules
- `src/service.rs` defines `ExecutionService`, session storage, async submit/wait/query, uploads, cancellation, and idle timeout.
- `src/session.rs` builds DataFusion `SessionContext`, registers UDFs/UDAFs/UDTFs, and installs custom planners/rules.
- `src/query.rs` owns SQL statement post-processing, DDL/DML/SHOW/COPY handling, current database/schema resolution, and query execution.
- `src/datafusion/` contains custom logical/physical rules, type/query planners, session rewrites, and MERGE support.
- `src/running_queries.rs` tracks running queries, request IDs, cancellation tokens, and completion notifications.
- `src/snowflake_error.rs`, `src/error.rs`, and `src/error_code.rs` preserve client-facing error contracts.
- `src/test_helpers.rs` creates local/offline in-memory Iceberg sessions used by executor and sqllogictest tests.
- `src/tests/sql/` holds snapshot-style coverage for SQL commands, DDL/DML, analyzers, optimizers, and functions.

## Local verification
- `cargo test -p executor`
- Narrow SQL snapshot filters: `cargo test -p executor -- query_`
- Focused unit filters: `cargo test -p executor -- test_update_all_table_names_visitor`
- For compatibility fallout, run the offline harness against a relevant corpus path: `cargo test -p embucket-sqllogictest -- sql-reference-functions/Date_ --test-threads 1`

## Common failure modes
- Reordering AST visitors in `UserQuery::postprocess_query_statement_with_validation` without checking parser snapshots.
- Bypassing `RunningQueriesRegistry` and breaking retry, wait, or cancel behavior.
- Using synchronous DataFusion catalog registration for Iceberg create/drop paths that require async catalog calls.
- Returning raw DataFusion errors where Snowflake-shaped REST clients expect mapped error codes.
- Adding a SQL feature without a focused executor or sqllogictest case.
