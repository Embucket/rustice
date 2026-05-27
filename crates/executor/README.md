# executor

The core query execution engine for Embucket, built on DataFusion. It handles query parsing, planning, optimization, and execution across various data sources.

## Purpose

This crate is central to Embucket's data processing capabilities. It leverages Apache DataFusion to execute SQL queries against configured catalogs and data sources.

## Query pipeline

`CoreExecutionService` implements the `ExecutionService` trait; each session is a
`UserSession` wrapping a DataFusion `SessionContext`. A SQL statement flows through
`UserQuery` (`src/query.rs`):

```
parse (forked sqlparser, snowflake dialect)
  → logical plan (DataFusion statement_to_plan)
  → custom analyzer rules
  → physical plan (CustomQueryPlanner + extension planner)
  → execute + collect → Vec<RecordBatch>
```

### Snowflake SQL dialect

The DataFusion `sql_parser.dialect` defaults to `"snowflake"` (`src/session.rs`), so the
forked sqlparser accepts Snowflake syntax including `MERGE INTO`, `COPY INTO`, `PIVOT` /
`UNPIVOT`, `LIKE`/`ILIKE ANY`, and `TOP N`.

### Custom types

`src/datafusion/type_planner.rs` adds Snowflake types: `VARIANT` → Utf8, `OBJECT` → Struct,
`NUMBER` → Decimal, and the `TIMESTAMP_LTZ` / `TIMESTAMP_TZ` family.

### Analyzer & optimizer rules (`src/datafusion/`)

Registered via `analyzer_rules(...)` and `with_optimizer_rule(...)` in `src/session.rs`:

- **Logical analyzers** — `like_type_analyzer` (LIKE/ILIKE coercion),
  `custom_type_coercion`, `iceberg_types_analyzer` (UInt → Int casts),
  `cast_analyzer` (session-aware casting), `union_schema_analyzer`.
- **Logical optimizer** — `split_ordered_aggregates`.
- **Physical optimizers** — `case_insensitive_schema`, `eliminate_empty_datasource_exec`,
  `remove_exec_above_empty`, `list_field_metadata`.

### MERGE INTO

`MERGE INTO` is modeled as a user-defined logical node (`logical_plan/merge.rs`) routed by
`extension_planner.rs` to a copy-on-write physical sink (`physical_plan/merge.rs`,
`MergeIntoCOWSinkExec`) that writes back to the Iceberg table. `COPY INTO` and the various
DDL/DML/`SHOW`/`EXPLAIN` statements are handled in `src/query.rs`.

### Error mapping

`src/error.rs` (snafu, with `#[error_stack_trace::debug]`) is translated by
`snowflake_error.rs` / `error_code.rs` into Snowflake numeric error codes (e.g. 2003 SQL
error, 2043 object-not-found, 630 timeout, 684 cancelled) so clients see Snowflake-shaped errors.

> **Routing note:** all queries currently execute **locally** against the Iceberg catalog.
> Offline/test execution is available via `test_helpers::create_df_session*`. There is no
> Snowflake passthrough/fallback in this crate yet (an intended contract per `AGENTS.md`).

## Async Query Execution
Query submitted asynchronously with fn `submit_query` returns AsyncQueryHandle which can be used with fn `wait_submitted_query_result` to consume query result. Underneath that two functions use `tokio::oneshot::channel` to communicate with each other.

## Historical Query Result
Multiple listeners can request result of running query with fn `wait_historical_query_result`. In opposite to polling which just returns status, it returns historical result if query isn't running anymore, or will wait for it to finish. As soon as query is finished and stored in history, listeners will get query result.
To make this happen `tokio::watch::channel` is used underneath for notifying listeners about query result status changes.

## Abort Query
Query can be aborted with fn `abort_query`.
Also SQL interface exposes `SYSTEM$CANCEL_QUERY` udf for aborting query by query UUID provided as string.
``` sql
SELECT SYSTEM$CANCEL_QUERY('123e4567-e89b-12d3-a456-426614174000');
```

## Running Queries Registry
`struct RunningQueriesRegistry` used for storing running queries info like cancellation token and Sender / Recever handles of watch channel. `trait RunningQueries` provides some interface for managing. This interface is used by ExecutionService and by `SYSTEM$CANCEL_QUERY` udf for queries aborting.
