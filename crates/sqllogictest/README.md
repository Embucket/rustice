# embucket-sqllogictest

A [sqllogictest][slt] harness that runs the embucket Snowflake-compatibility
corpus against rustice's in-process SQL engine.

It is **not** part of the main `cargo test` suite. It is a tracked
compatibility report: failures are expected while rustice grows its Snowflake
surface, and the CI job is configured to be non-gating.

---

## Quick start

```bash
# Run the bronze_scope corpus (~340 files) in parallel.
cargo test -p embucket-sqllogictest

# Filter to a substring of the path.
cargo test -p embucket-sqllogictest -- listagg variant

# List the files that would run, then exit.
cargo test -p embucket-sqllogictest -- --list

# Exit non-zero if any file fails (otherwise the process always exits 0).
cargo test -p embucket-sqllogictest -- --strict

# Also run the vendored Databend corpus.
cargo test -p embucket-sqllogictest -- --include-databend

# Tune parallelism (defaults to logical CPU count).
cargo test -p embucket-sqllogictest -- --test-threads 4
```

Output ends with a per-directory pass/fail summary followed by a list of
failing files (truncated to the first three error lines each):

```
===== sqllogictest summary =====
  bronze_scope/data-types                                  pass=3    fail=5
  bronze_scope/sql-reference-functions/Aggregate           pass=8    fail=17
  …
  TOTAL                                                    pass=142  fail=198  (87421 ms)

--- failing files ---
  bronze_scope/sql-reference-functions/Aggregate/listagg.slt (2 error(s))
    - query result mismatch:
    - query result mismatch:
  …
```

---

## Layout

```
crates/sqllogictest/
├── Cargo.toml                  # name = "embucket-sqllogictest"
├── README.md                   # this file
├── dev/
│   └── sync-slt.sh             # re-sync the vendored corpus from embucket-labs
├── src/
│   ├── lib.rs                  # module roots + `embucket_validator`
│   ├── conversion.rs           # cell-level string helpers (verbatim from DataFusion)
│   ├── output.rs               # `DFColumnType` ColumnType impl (verbatim from DataFusion)
│   ├── normalize.rs            # RecordBatch → Vec<Vec<String>> (trimmed from DataFusion)
│   ├── preprocessor.rs         # strip embucket-specific directives
│   ├── engine.rs               # `EmbucketSession` — `AsyncDB` adapter over `UserSession`
│   └── error.rs                # thiserror enum
└── tests/
    ├── sqllogictests.rs        # binary entry point (`harness = false`)
    └── slt/
        ├── bronze_scope/       # vendored from embucket-labs/test/sql/bronze_scope/
        └── databend/           # vendored from embucket-labs/test/sql/databend/
```

The `tests/sqllogictests.rs` binary is registered with `harness = false`, so
`cargo test -p embucket-sqllogictest` invokes it directly and passes through
any flags that follow `--`.

---

## How it works

1. **Discover** every `.slt` file under `tests/slt/`. Apply the path-substring
   filters from the CLI. Skip `databend/` unless `--include-databend` is set.

2. **For each file**, in parallel up to `--test-threads`:
   - Read the file into memory.
   - Run [`preprocessor::strip_custom_directives`](src/preprocessor.rs) over
     the text to remove embucket-specific directives the upstream parser does
     not understand: `exclude-from-coverage`, `skip-if`, `only-if`.
   - Parse with `sqllogictest::parse_with_name`.
   - Build a fresh `Arc<UserSession>` via
     `executor::test_helpers::create_df_session_with_catalog_url("/dev")` —
     an in-memory Iceberg catalog with the `embucket.public` schema and the
     standard fixture tables (`employee_table`, `department_table`, etc.)
     pre-created. Sessions are never shared across files.
   - Build a `sqllogictest::Runner` wrapping an `EmbucketSession` adapter.
     Register the engine label `embucket` and the
     [`embucket_validator`](src/lib.rs) cell comparator.
   - Drive records one at a time. Errors are collected (capped per file at
     `ERRS_PER_FILE_LIMIT = 10`) rather than aborting the run.

3. **Aggregate** results into a per-directory `pass/fail` summary printed to
   stderr. Exit non-zero only if `--strict` is set.

### The engine adapter

[`EmbucketSession`](src/engine.rs) implements `sqllogictest::AsyncDB` by
forwarding to `UserSession::query(sql, QueryContext::default()).execute()`.
The returned `QueryResult { records, schema, … }` is converted to the
`Vec<Vec<String>>` form sqllogictest expects via
[`normalize::convert_batches`](src/normalize.rs) and
[`normalize::convert_schema_to_types`](src/normalize.rs) — adapted verbatim
from DataFusion's harness so float/decimal formatting matches the corpus
authoring conventions.

### Custom directives

The embucket Python runner accepts three directives the Rust
`sqllogictest = "0.29"` parser doesn't:

| Directive               | Meaning in the Python runner            | What we do            |
| ----------------------- | --------------------------------------- | --------------------- |
| `exclude-from-coverage` | Don't count this block toward coverage. | Dropped (line-level). |
| `skip-if <…>`           | Conditional skip.                       | Dropped (line-level). |
| `only-if <…>`           | Conditional run.                        | Dropped (line-level). |

Upstream `onlyif`/`skipif` (no hyphen) are left untouched and respected by
the upstream parser.

### Regex expected values

Some cells in the corpus are expressed as
`<REGEX>:<rust regex pattern>` to handle non-deterministic output (e.g.
`LISTAGG(DISTINCT …)` ordering). [`embucket_validator`](src/lib.rs)
recognises the prefix and compiles the rest as a `regex::Regex`, applying it
to the tab-joined actual row. All other cells are compared verbatim.

### Failure mode

Soft by default. The harness exits 0 even when files fail; CI is configured
likewise (`continue-on-error: true`). Pass `--strict` to flip this and exit
non-zero whenever any file has at least one error.

---

## Adding or editing `.slt` files

The corpus is vendored from `embucket-labs`. Two ways to update it:

**Targeted edits.** Edit a file in `tests/slt/…` directly. Commit the
change. The next harness run picks it up.

**Bulk re-sync from embucket-labs.** Check out `embucket-labs` next to
`rustice` (or set `EMBUCKET_LABS` to its path), then:

```bash
bash crates/sqllogictest/dev/sync-slt.sh
# review `git diff` carefully before committing
```

The script uses `rsync --delete`, so files removed upstream are also removed
locally.

---

## Investigating failures

For a single failing file, the most useful command is:

```bash
cargo test -p embucket-sqllogictest -- path/to/file.slt --test-threads 1
```

The harness prints the first three errors per file in the summary. To see
the full error trail for a file, drop `--test-threads` or grep the file's
section in the stderr output.

Common failure shapes:

- **`query result mismatch:`** — rustice executed the query successfully but
  the returned rows don't match the expected block. Either rustice is wrong,
  or the expected output was authored against Snowflake/DataFusion behavior
  that rustice does not match. Verbatim comparison is intentional; treat
  these as the headline compatibility signal.

- **`statement is expected to fail, but actually succeed:`** — rustice ran a
  statement that the corpus expects to error. Often a missing validation in
  the engine.

- **`statement failed: <executor message>` against expected
  `statement error "…"`** — the engine errored but its message doesn't
  match. The corpus uses Snowflake-style codes (`100069 (22P02): …`) that
  rustice will never reproduce verbatim. Most error-text tests will fail
  here until rustice mirrors Snowflake error wording (or the test file is
  edited to use rustice's wording).

- **`parse error:`** — the file uses syntax the upstream parser doesn't
  accept and the preprocessor doesn't strip. Inspect the file; if it's a
  new embucket directive, extend `STRIP_PREFIXES` in
  [`preprocessor.rs`](src/preprocessor.rs).

---

## CI

`.github/workflows/tests.yml` defines a `sqllogictest (non-gating)` job that
runs:

```bash
cargo test -p embucket-sqllogictest --profile=ci --test sqllogictests -- \
  --test-threads $(nproc)
```

with `continue-on-error: true`. The main `required` test job enumerates the
other workspace crates with `-p` so the harness isn't built twice.

---

## Why a separate harness instead of reusing embucket-labs's Python runner?

The Python runner at `embucket-labs/test/slt_runner/` drives the embucket
binary over the Snowflake REST wire protocol. This harness drives rustice
**in-process** via the `executor` crate's `UserSession`, which means:

- No network, no server boot, no port allocation — runs as a normal
  `cargo test` target.
- Errors include native Rust backtraces, not just JSON-over-HTTP error
  strings.
- Each `.slt` file gets a fresh isolated catalog instead of sharing the
  server's global state.

The Python runner remains the right tool when testing the full embucket
stack (REST API, sessions, auth). This harness is the right tool when
testing the SQL engine itself.

[slt]: https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki
