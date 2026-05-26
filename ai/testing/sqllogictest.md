# Sqllogictest

## Role in this repo

`embucket-sqllogictest` is the offline compatibility harness for Rustice's SQL engine. It runs `.slt` files against an in-process `executor::UserSession`, not the Snowflake REST API and not Snowflake.

Use it when a change affects user-visible SQL behavior across parser, planner, execution, functions, types, casts, nulls, timestamps, or compatibility corpus expectations.

The harness is intentionally a compatibility report. By default it exits 0 even when files fail; use `--strict` when you need failures to fail the process.

## Where files live

- Harness crate: `crates/sqllogictest`
- Test binary: `crates/sqllogictest/tests/sqllogictests.rs`
- Main corpus: `crates/sqllogictest/tests/slt/bronze_scope/`
- Opt-in Databend corpus: `crates/sqllogictest/tests/slt/databend/`
- Engine adapter: `crates/sqllogictest/src/engine.rs`
- Normalization: `crates/sqllogictest/src/normalize.rs`, `conversion.rs`, `output.rs`
- Custom directive stripping: `crates/sqllogictest/src/preprocessor.rs`
- `<REGEX>:` expected-value validator: `crates/sqllogictest/src/lib.rs`

Each `.slt` file gets a fresh local session from `executor::test_helpers::create_df_session_with_catalog_url("/dev")`.

## How to run

List matching files:

```bash
cargo test -p embucket-sqllogictest -- --list
```

Run the default `bronze_scope` corpus:

```bash
cargo test -p embucket-sqllogictest
```

Run path-substring filters:

```bash
cargo test -p embucket-sqllogictest -- sql-reference-functions/Conversion
cargo test -p embucket-sqllogictest -- data-types/datetime --test-threads 1
```

Exit non-zero on any failing file:

```bash
cargo test -p embucket-sqllogictest -- --strict
```

Include the opt-in Databend corpus:

```bash
cargo test -p embucket-sqllogictest -- --include-databend
```

CI runs the harness as a non-gating job with:

```bash
cargo test -p embucket-sqllogictest --profile=ci --test sqllogictests -- --test-threads $(nproc)
```

## What belongs in sqllogictest

Add or update `.slt` cases for SQL-visible behavior where a user would care about the statement result or error:

- end-to-end expression behavior across parser, planner, and execution
- compatibility examples for functions, casts, data types, DDL/DML, and query syntax
- regression cases involving multiple engine layers
- result formatting and ordering cases that are easier to read as SQL
- known compatibility gaps that should remain tracked as corpus failures

Use Rust unit/snapshot tests instead for:

- isolated helper functions or parser visitors
- exact Rust error variants
- internal planner structures where SQL output is not the best assertion
- REST wire response shape
- session/JWT/cookie extraction
- catalog/metastore model invariants

Many changes need both: a focused Rust test to pin the implementation and an `.slt` case to track user-visible compatibility.

## Compatibility guidance

### NULL behavior

Add rows covering null input, null output, mixed null/non-null rows, and nulls inside arrays/objects where relevant. Do not assume DataFusion null behavior matches Snowflake; assert the intended Rustice behavior.

### Casts

Cover valid casts, invalid casts, try-cast style behavior if applicable, boundary values, and strings that look numeric/date-like. Cast tests often belong in both executor snapshots and `data-types/` or `sql-reference-functions/Conversion` `.slt` files.

### Timestamp and date behavior

Include precision, timezone-like inputs, date/timestamp casts, interval/date arithmetic, boundary-count behavior, and session-parameter-sensitive behavior when applicable. Prefer deterministic literal timestamps and dates.

### Functions

For a function implementation, first add focused `functions` tests. Add sqllogictest coverage when the behavior is part of the compatibility corpus or combines parsing, planning, and execution. Include aliases, arity errors, nulls, type coercion, and table input when relevant.

### Parser and planner edge cases

Use `.slt` when parser/planner changes affect accepted SQL or final results. Keep internal visitor transformations in Rust tests unless the important contract is the user-visible query behavior.

### Result ordering

Add `ORDER BY` whenever row order is semantically important. If output is intentionally nondeterministic, use the existing `<REGEX>:` validator narrowly for the affected cells instead of broadening normalization.

## How to add a new case

1. Choose the closest file under `crates/sqllogictest/tests/slt/bronze_scope/`.
2. Add the smallest SQL block that proves the behavior.
3. Include setup statements in the same file when the case needs tables.
4. Prefer deterministic values and explicit `ORDER BY`.
5. Run one filtered path:

   ```bash
   cargo test -p embucket-sqllogictest -- path/to/file.slt --test-threads 1
   ```

6. If the file is expected to fail today, keep the failure intentional and explain it in the PR or related issue. Use `--strict` only when you want the local run to fail on corpus failures.
7. If the behavior comes from a Rust change, also run the owning crate's focused test.
