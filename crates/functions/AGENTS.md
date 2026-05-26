# functions agent guide

## What this crate owns
- Snowflake-compatible SQL function implementations for DataFusion: scalar UDFs, aggregate UDFs, window UDFs, and table functions.
- Function registration, aliases, signatures, type coercion, null handling, return-type inference, and session-parameter-aware behavior.
- SQL AST visitors that rewrite Snowflake syntax into forms the executor/DataFusion planner can handle, plus unimplemented-function detection.
- Function snapshot tests and helper docs/templates for adding new functions.

## What this crate must not own
- Query execution orchestration, session lifecycle, catalog DDL, or REST protocol behavior.
- DataFusion catalog/provider wrappers or metastore state.
- Snowflake passthrough/fallback routing; function gaps should be surfaced through tests or explicit unsupported-function errors.
- Activating geospatial code without verifying the currently disabled registration path in `src/lib.rs`.

## Important files and modules
- `src/lib.rs` declares active modules and registers UDFs/UDAFs/UDTFs/window functions.
- Category modules such as `conditional/`, `conversion/`, `datetime/`, `semi-structured/`, `string-binary/`, `regexp/`, `aggregate/`, `table/`, and `window/` hold implementations.
- `src/expr_planner.rs` handles custom expression planning.
- `src/visitors/` rewrites Snowflake-specific SQL constructs; `src/visitors/unimplemented/` tracks function coverage.
- `src/session_params.rs` defines session properties used by conversion/date/time behavior.
- `src/tests/` contains snapshot-style function and visitor tests.
- `docs/function_implementation_guide.md` and `src/scalar_template.rs` document the local implementation pattern.

## Local verification
- `cargo test -p functions`
- Narrow function snapshots: `cargo test -p functions -- query_to_date`
- Visitor coverage: `cargo test -p functions -- visitors`
- Local/offline compatibility check for touched functions: `cargo test -p embucket-sqllogictest -- sql-reference-functions/Conversion --test-threads 1`

## Common failure modes
- Implementing a function but not registering it in `src/lib.rs` or the category `mod.rs`.
- Updating coverage data but not keeping the unimplemented-function tracker in sync.
- Using raw Arrow/DataFusion types where the local guide expects logical types and Snowflake coercion semantics.
- Mishandling scalar vs array inputs, null propagation, or Utf8/Utf8View differences.
- Changing function output formatting without updating snapshots and any relevant sqllogictest expectations.
