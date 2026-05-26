# functions

Defines and registers Snowflake-compatible SQL functions that extend the DataFusion query
engine used within Embucket.

## Purpose

This crate implements scalar, aggregate, window, and table functions that match Snowflake
semantics (signatures, type coercion, return types) and are not part of the standard
DataFusion distribution.

## For Contributors

📖 **[Functions Implementation Guide](docs/function_implementation_guide.md)** — complete guide for implementing functions in Embucket

🔧 **[Function Template](src/scalar_template.rs)** — ready-to-use template for creating new scalar functions

## Registration

Functions are registered into a DataFusion `FunctionRegistry` from `executor`'s session
setup (`crates/executor/src/session.rs`):

- `register_udfs(registry, session_params)` — scalar UDFs for every enabled category (`src/lib.rs:53`).
- `register_udafs(registry)` — aggregate UDFs (re-exported from `aggregate`).
- `register_udtfs(&ctx)` — table functions (`functions::table`, currently `FLATTEN`).
- `window::register_udwfs(registry)` — window UDFs.

Scalar/aggregate singletons are built with the `make_udf_function!` / `make_udaf_function!`
macros; `expr_planner.rs` adds custom expression planning (e.g. `SUBSTRING` → `SUBSTR`).

## Categories

Each category is a module under `src/`. The following are **registered and active**:

| Category | Module | Examples |
|----------|--------|----------|
| Conditional | `conditional` | IFF, EQUAL_NULL, NULLIFZERO, ZEROIFNULL, BOOLAND/BOOLOR/BOOLXOR |
| Conversion | `conversion` | TO_VARIANT, TO_DECIMAL, TO_DATE, TO_TIMESTAMP*, TO_VARCHAR (+ `try_*`) |
| Crypto | `crypto` | MD5 |
| Date & Time | `datetime` | DATE_ADD, DATE_DIFF, *_FROM_PARTS, CONVERT_TIMEZONE, LAST_DAY |
| Numeric | `numeric` | DIV0, TRY_DIV0 |
| Encryption | `encryption` | ENCRYPT_RAW, DECRYPT_RAW |
| String & Binary | `string-binary` | LENGTH, SUBSTR, SPLIT, SHA2, HEX_ENCODE/DECODE, PARSE_IP, JAROWINKLER_SIMILARITY |
| RegExp | `regexp` | REGEXP_SUBSTR, REGEXP_REPLACE, REGEXP_INSTR, REGEXP_LIKE |
| Semi-structured | `semi-structured` | ARRAY_*, OBJECT_*, VARIANT, PARSE_JSON, GET, GET_PATH, TYPEOF (largest category) |
| Session / Context | `session` | CURRENT_DATABASE, CURRENT_SCHEMA, CURRENT_WAREHOUSE, CURRENT_ROLE, LAST_QUERY_ID |
| System | `system` | TYPEOF, SYSTEM$CANCEL_QUERY, SLEEP |
| Aggregate | `aggregate` | LISTAGG, OBJECT_AGG, BOOLAND_AGG, PERCENTILE_CONT, ARRAY_*_AGG |
| Window | `window` | CONDITIONAL_TRUE_EVENT |
| Table | `table` | FLATTEN |

JSON functions are provided via the `datafusion-functions-json` fork and registered
alongside the above.

### Not implemented / disabled

- **Geospatial** (`src/geospatial/`) is present but **disabled** in `src/lib.rs` (commented
  out as a workaround for a non-working dependency under `cargo test --all-features`).
- Large parts of the Snowflake catalog are **not yet implemented** — notably most system,
  file/stage, notification, generation, vector, bitwise, and information-schema functions.

## Snowflake coverage tracking

`src/visitors/unimplemented/` enumerates the full Snowflake function catalog (~500 functions)
by category in `generated_snowflake_functions.rs`, and `functions_checker.rs` /
`functions_list.rs` drive gap analysis between that catalog and what this crate implements
(roughly ~130 functions today). `to_snowflake_datatype()` (`src/lib.rs`) maps Arrow types to
Snowflake type names for result metadata.

## Testing

Functions use snapshot tests via the `test_query!` macro (`src/tests/`).
