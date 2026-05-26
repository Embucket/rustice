# Test matrix

Offline/local testing is the default loop. Snowflake-backed testing is an additional oracle or integration layer for behavior that cannot be proven locally.

Use the narrowest required command first, then widen based on blast radius. Commands listed here are repository-local unless marked otherwise.

| Change type | Required local verification | Add sqllogictest when | Additional notes |
| --- | --- | --- | --- |
| Scalar function behavior | `cargo test -p functions -- <function_filter>` | The behavior is Snowflake compatibility visible or covered by `crates/sqllogictest/tests/slt/bronze_scope/sql-reference-functions/` | Include nulls, aliases, arity/type errors, scalar and table inputs where relevant. |
| Aggregate/window/table function behavior | `cargo test -p functions -- <module_filter>` and, when planner interaction matters, `cargo test -p executor -- <query_filter>` | SQL-visible aggregate/window/table behavior changes | Table functions such as `FLATTEN` often need executor coverage because planning matters. |
| Timestamp/date behavior | `cargo test -p functions -- <date_or_timestamp_filter>` and `cargo test -p executor -- timestamp` | `data-types/datetime` or `sql-reference-functions/Date_` cases are affected | Cover precision, casts, boundary behavior, and session-sensitive behavior. |
| Parser/planner behavior | `cargo test -p executor -- <command_or_analyzer_filter>` | Accepted SQL or final results change for compatibility corpus statements | Good filters include command names such as `top`, `fetch`, `pivot`, `unpivot`, or analyzer names such as `like_type_analyzer`. |
| Execution behavior | `cargo test -p executor -- <query_filter>` | The behavior is user-visible across full SQL statements | Include setup queries through the executor `test_query!` macro. Use local temp files for file/COPY behavior. |
| API response shape | `cargo test -p api-snowflake-rest test_rest_api -- --test-threads=1` | Usually no; sqllogictest bypasses REST | Also run `cargo test -p api-snowflake-rest test_gzip_encoding -- --test-threads=1` for compression/body handling. |
| Session behavior | `cargo test -p api-snowflake-rest-sessions -- session` and, for REST-visible session effects, `cargo test -p api-snowflake-rest test_rest_api -- --test-threads=1` | SQL session variables affect query results | Check JWT audience, cookie-only sessions, SPCS ingress headers, redaction, and session expiry. |
| Catalog/table discovery | `cargo test -p catalog -- information_schema` and/or `cargo test -p executor -- query_show` | SQL corpus metadata behavior changes | For metastore model changes, add `cargo test -p catalog-metastore`. |
| DDL/DML/table writes | `cargo test -p executor -- query_create_table`, `cargo test -p executor -- query_merge`, or another focused query filter | Compatibility corpus DDL/DML cases are touched | Local writes are valid local behavior when supported; do not force read-only as the default. |
| Routing/fallback behavior | Current state: no complete passthrough/fallback router is visible. Required local verification is a focused classifier/routing test once such code exists. Candidate command: `cargo test -p api-snowflake-rest -- <routing_filter>` or `cargo test -p executor -- <routing_filter>`; verify before relying on this. | Add `.slt` only if local query behavior changes; routing itself likely needs Rust tests | Must prove no silent fallback. Snowflake-backed testing is required only when actual remote passthrough/fallback is implemented. |
| Performance-sensitive execution | Focused correctness test first: `cargo test -p executor -- <query_filter>`. Candidate for local benchmarking: add or run a repository benchmark only after verifying one exists. | Usually no, unless result behavior changes | Snapshot filters normalize timing metrics; do not use snapshots as performance proof. |
| Error mapping | `cargo test -p executor -- snowflake_errors` or focused executor snapshot with `snowflake_error = true`; REST errors: `cargo test -p api-snowflake-rest test_rest_api -- --test-threads=1` | Error behavior is represented in corpus files | Keep local error contracts explicit; do not hide differences with broad regexes. |
| REST auth/compression/body handling | `cargo test -p api-snowflake-rest test_gzip_encoding -- --test-threads=1`; auth/session extraction may also need `cargo test -p api-snowflake-rest-sessions` | No | These tests start local servers; they do not require Snowflake. |
| Catalog-metastore models or volumes | `cargo test -p catalog-metastore` | Only if SQL-visible catalog behavior changes | S3/networked tests are not part of the default loop. Use memory or file fixtures first. |
| Docs-only changes | `git diff --check` | No | If docs include commands, verify command spelling against `Cargo.toml` or label uncertain commands as candidate. |

## Wider local loops

Use these after focused tests when the change crosses crate boundaries:

```bash
cargo test -p functions
cargo test -p executor
cargo test -p catalog
cargo test -p api-snowflake-rest -- --test-threads=1
cargo test -p api-snowflake-rest-sessions
cargo test -p embucket-sqllogictest -- <path-substring> --test-threads 1
```

CI's main local Rust test shape is:

```bash
cargo test --profile=ci --workspace --all-targets --exclude api-snowflake-rest --exclude embucket-sqllogictest
cargo test --profile=ci -p api-snowflake-rest --all-targets -- --test-threads=1
```

The sqllogictest CI job is non-gating:

```bash
cargo test -p embucket-sqllogictest --profile=ci --test sqllogictests -- --test-threads $(nproc)
```

## External or optional loops

- Ignored executor tests that mention S3 or public buckets require explicit network/credentials and are not part of the default development loop.
- The root `Makefile` has `integration-test`, but this checkout does not contain a root `tests/` directory. Treat that target as candidate infrastructure and verify before relying on it.
- Snowflake oracle tests are additional evidence for exact Snowflake behavior or remote routing. They should not be required for ordinary local engine changes.
