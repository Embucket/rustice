# embucket-sqllogictest

A [sqllogictest][slt] harness that runs rustice's first-party Snowflake-
compatibility corpus against the in-process SQL engine.

It is **not** part of the main `cargo test` suite. It is a tracked
compatibility report: failures are expected while rustice grows its Snowflake
surface, and the CI job is configured to be non-gating.

---

## Quick start

```bash
# Run every .slt file under tests/slt/, in parallel.
cargo test -p embucket-sqllogictest

# Filter to a substring of the path.
cargo test -p embucket-sqllogictest -- listagg variant

# Run only the dbt-snowplow-web compat suite.
cargo test -p embucket-sqllogictest -- dbt_snowplow_web

# List the files that would run, then exit.
cargo test -p embucket-sqllogictest -- --list

# Exit non-zero if any file fails (otherwise the process always exits 0).
cargo test -p embucket-sqllogictest -- --strict

# Tune parallelism (defaults to logical CPU count).
cargo test -p embucket-sqllogictest -- --test-threads 4

# Write a markdown report of the run (per-directory table + full diffs).
cargo test -p embucket-sqllogictest -- --report /tmp/slt.md
```

Each file emits a `[N/total] PASS|FAIL` line as it completes, and the run
ends with a per-directory pass/fail summary followed by full error bodies
for every failing file (SQL + expected/actual diff).

---

## Layout

```
crates/sqllogictest/
├── Cargo.toml                  # name = "embucket-sqllogictest"
├── README.md                   # this file
├── dev/
│   ├── regen-snowplow-slt.sh   # regenerate dbt_snowplow_web/*.slt leaf files
│   └── regen-snowplow-setup.sh # regenerate fixtures/snowplow/setup.slt
│                               # (full-refresh CTAS + incremental INSERT/MERGE chain)
├── src/
│   ├── lib.rs                  # module roots + `embucket_validator`
│   ├── conversion.rs           # cell-level string helpers (verbatim from DataFusion)
│   ├── output.rs               # `DFColumnType` ColumnType impl (verbatim from DataFusion)
│   ├── normalize.rs            # RecordBatch → Vec<Vec<String>> (trimmed from DataFusion)
│   ├── engine.rs               # `EmbucketSession` — `AsyncDB` adapter over `UserSession`
│   └── error.rs                # thiserror enum
└── tests/
    ├── sqllogictests.rs        # binary entry point (`harness = false`)
    ├── fixtures/
    │   └── snowplow/           # TSV data + setup.slt for the dbt_snowplow_web suite
    └── slt/
        ├── bronze_scope/       # data-types, sql-reference-commands, functions
        ├── databend/           # auxiliary corpus
        └── dbt_snowplow_web/   # 18 dbt-snowplow-web models × incremental + full_refresh
```

The `tests/sqllogictests.rs` binary is registered with `harness = false`, so
`cargo test -p embucket-sqllogictest` invokes it directly and passes through
any flags that follow `--`.

---

## How it works

The harness is intentionally thin — file discovery, parallel scheduling, and
result aggregation are the only meaningful pieces of glue. Parsing,
`include` resolution, and variable substitution are handled by the upstream
[`sqllogictest`][slt-crate] crate.

1. **Discover** every `.slt` file under `tests/slt/`. Apply the path-substring
   filters from the CLI.

2. **For each file**, in parallel up to `--test-threads`:
   - Parse with [`sqllogictest::parse_file`][parse_file] — the parser
     handles `include` directives natively (glob-based, resolved relative
     to the including file).
   - Build a fresh `Arc<UserSession>` via
     `executor::test_helpers::create_df_session_with_catalog_url(...)` —
     either an in-memory `/dev` catalog or, for fixture-loading suites
     (see `FILE_CATALOG_SUITES`), a per-file `tempfile::tempdir()` `file://`
     catalog. Sessions are never shared across files.
   - Build a `sqllogictest::Runner` wrapping an `EmbucketSession` adapter.
     Register the engine label `embucket`, the [`embucket_validator`](src/lib.rs)
     cell comparator, and the `CRATE_ROOT` variable (for `${CRATE_ROOT}`
     substitution).
   - Drive records one at a time. Errors are collected (capped per file at
     `ERRS_PER_FILE_LIMIT = 10`) rather than aborting the run.

3. **Aggregate** results into a per-directory `pass/fail` summary printed to
   stderr. Exit non-zero only if `--strict` is set.

### The engine adapter

[`EmbucketSession`](src/engine.rs) implements `sqllogictest::AsyncDB` by
forwarding to `UserSession::query(sql, QueryContext::default()).execute()`.
The returned `QueryResult` is converted to the `Vec<Vec<String>>` form
sqllogictest expects via [`normalize::convert_batches`](src/normalize.rs)
and [`normalize::convert_schema_to_types`](src/normalize.rs) — adapted
verbatim from DataFusion's harness so float/decimal formatting matches
the corpus authoring conventions.

### Variable substitution

The harness publishes `CRATE_ROOT` (absolute path to this crate's manifest
directory) to the runner via `Runner::set_var`. A `.slt` file can opt in to
substitution with the upstream `control substitution on/off` directive and
then reference `${CRATE_ROOT}` inside the bracketed region. The
[snowplow setup](tests/fixtures/snowplow/setup.header.slt) uses this to
reach the committed TSV fixture.

Substitution is **off by default**; it must be bracketed because the
upstream `subst` parser treats `\` as an escape character and would mangle
literal SQL backslashes (e.g. `'\t'` field delimiters) outside the
substitution region.

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

## Editing `.slt` files

Edit a file in `tests/slt/…` directly. Commit the change. The next harness
run picks it up. The corpus is first-party — there is no upstream to sync
from.

---

## Investigating failures

For a single failing file, the most useful command is:

```bash
cargo test -p embucket-sqllogictest -- path/to/file.slt --test-threads 1
```

The summary section prints all errors per file (capped at 10). Common
failure shapes:

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

- **`parse error:`** — the file uses syntax the upstream `sqllogictest`
  parser rejects. Most common cause: a leading-whitespace `# comment` or a
  bare non-standard directive. Edit the file to make it parser-clean.

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

## dbt-snowplow-web compat suite

`tests/slt/dbt_snowplow_web/` contains 36 `.slt` files (18 dbt models × incremental
and full_refresh modes) that run the compiled dbt-snowplow-web SQL verbatim
against a session pre-loaded with a 200-row slice of canonical Snowplow events
and the full dbt-snowplow-web DAG materialised.

Path-based dispatch (`FILE_CATALOG_SUITES` in `tests/sqllogictests.rs`)
switches these files onto a per-file `tempfile::tempdir()` `file://` catalog
so `COPY INTO file://` can reach the TSV fixture; all other suites stay on
the in-memory `/dev` catalog.

Each leaf `.slt` does `include ../../../fixtures/snowplow/setup.slt`; the
upstream parser resolves relative include paths against the including
file's directory automatically. `setup.header.slt` wraps its `COPY INTO`
with `control substitution on/off` and reads the TSV via
`'file://${CRATE_ROOT}/tests/fixtures/snowplow/events.csv'` (tab-delimited
content; the `.csv` extension is required for the listing-table filter).

`setup.slt` materialises the dbt-snowplow-web DAG by, for each of the 18
models in dependency order, running its full-refresh SQL as `CREATE TABLE AS`
to lay down the schema and then either `INSERT INTO` (for `+materialized: table`
models) or the verbatim **MERGE INTO** that dbt-snowflake writes to
`target/run/` (for the 4 `+materialized: incremental` derived models:
`snowplow_web_sessions`, `_page_views`, `_users`, `_user_mapping`). Mirrors
how production dbt boots a cold warehouse and then runs an incremental cycle,
including the canonical incremental upsert.

Regenerate both the leaf files and `setup.slt` after the upstream dbt
compiler output changes:

```bash
bash crates/sqllogictest/dev/regen-snowplow-slt.sh    # the 36 leaf .slt files
bash crates/sqllogictest/dev/regen-snowplow-setup.sh  # tests/fixtures/snowplow/setup.slt
```

See `tests/fixtures/snowplow/README.md` for fixture provenance and how the
events table is materialised (mirrors `snowplow-events-parquet`'s
`sql/tsv_to_parquet.sql.tmpl`).

[slt]: https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki
[slt-crate]: https://crates.io/crates/sqllogictest
[parse_file]: https://docs.rs/sqllogictest/0.29/sqllogictest/fn.parse_file.html
