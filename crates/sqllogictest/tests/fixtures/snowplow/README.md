# Snowplow fixtures

Fixture data and setup SQL for the `dbt_snowplow_web` sqllogictest suite
(`crates/sqllogictest/tests/slt/dbt_snowplow_web/`).

## Files

- **`events1.parquet`**, **`events2.parquet`** — Two 200-row typed parquet
  fixtures copied from the upstream snowplow-events-parquet pipeline
  (`snowplow-events-parquet/runs/20260527T232659Z/parquet/data.parquet`
  and `…20260527T232712Z/…`). Each file has 133 columns matching the
  events table declaration: 131 source columns (typed TIMESTAMP / INT /
  DOUBLE / BOOLEAN / VARCHAR), the regex-extracted
  `contexts_com_snowplowanalytics_snowplow_web_page_1` column, and a
  baked `load_tstamp` (events1 = `2026-05-27 23:27:11`, events2 =
  `2026-05-27 23:27:24` — frozen at the upstream pipeline's `now()`).

  The two files have **disjoint event_ids** but share the **same 20
  sessions and 5 users** — they represent two batches of events arriving
  in roughly the same wall-clock window (2026-05-27 20:40 → 23:27 UTC),
  designed to exercise dbt-snowplow-web's late-arriving-events MERGE
  semantics.

- **`setup.header.slt`** — Hand-maintained bootstrap: schemas, the typed
  `events` table declaration + `COPY INTO` of `events1.parquet`, and
  empty stubs for the three dim seeds (`snowplow_web_dim_*`). No staging
  table, no runtime CTAS — the parquet files are already in the typed
  events shape. Column shapes for the dim stubs match the headers of the
  dbt package's seed CSVs at `dbt_packages/snowplow_web/seeds/*.csv`.

- **`setup.slt`** — Generated file (do not edit by hand). Concatenates
  `setup.header.slt` with a **two-phase** materialisation chain that
  simulates a real dbt operational cycle:

  **Phase A — cold start, events1 only:**
  For each of the 18 models, in DAG order:
    1. `CREATE TABLE <schema>.<model> AS <full-refresh SELECT>` — lays
       down the schema; the full-refresh `9999-01-01` sentinel timestamps
       mean the table is typically empty after this step.
    2. Branches on the model's canonical dbt materialisation:
       - **`+materialized: incremental`** (4 derived models: sessions,
         page_views, users, user_mapping): materialise a `<model>__dbt_tmp`
         source from the incremental SELECT, then run the **verbatim MERGE
         INTO** statement that dbt-snowflake's incremental materialisation
         writes to `target/run/.../<model>.sql`.
       - **`+materialized: table`** (everything else): `INSERT INTO
         <schema>.<model> <incremental SELECT>`.

  **Phase B — incremental cycle, events2 appended:**
    1. `COPY INTO events FROM events2.csv` — append the second batch of
       typed events directly to the events table (now 400 rows). No
       staging, no CTAS — events2.csv is already pre-baked typed.
    2. For each of the 18 models, in DAG order:
       - **`+materialized: incremental`** (4 derived models):
         `DROP TABLE <model>__dbt_tmp; CREATE TABLE <model>__dbt_tmp AS
         <incremental SELECT>; <verbatim MERGE INTO>` — upserts new rows
         into the persistent table.
       - **`+materialized: table`** (everything else):
         `DROP TABLE <model>; CREATE TABLE <model> AS <incremental SELECT>`
         — matches dbt's per-run rebuild of `_this_run` scratch tables.

  Phase B's incremental SQL re-scans the typed events table (now 400 rows)
  with the same window filter Phase A used; events2's events show up as
  new rows in the scratch tables, which then propagate to the derived
  tables via MERGE.

  This mirrors how dbt-snowplow-web actually boots a cold warehouse and
  then absorbs a follow-up incremental cycle. The MERGE phase in Phase B
  is the late-arriving-events upsert the four `+materialized: incremental`
  models are designed for.

## How it's wired in

Leaf `.slt` files pull setup in with a plain relative include:

```
include ../../../fixtures/snowplow/setup.slt
```

Upstream `sqllogictest::parse_file` resolves relative include paths against
the including file's directory.

The two `COPY INTO` statements (events1 in `setup.header.slt`, events2 in
`setup.slt`'s Phase B preamble) reference the fixtures through the upstream
variable-substitution mechanism:

```
control substitution on

statement ok
COPY INTO embucket.public_snowplow_manifest.events
FROM 'file://${CRATE_ROOT}/tests/fixtures/snowplow/events1.parquet'
FILE_FORMAT = ( TYPE = 'PARQUET' );

control substitution off
```

The harness publishes `CRATE_ROOT` to the runner via `Runner::set_var`
(value: `env!("CARGO_MANIFEST_DIR")`), so the committed `.slt` doesn't bake
in machine-specific paths. Substitution is bracketed because the upstream
`subst` parser treats `\` as an escape — outside the bracket, dbt-compiled
SQL contains literal `$` references that must not be substituted.

For `dbt_snowplow_web/` paths the harness builds a per-file `tempfile::tempdir()`
and passes `file://<tempdir>` as the Iceberg catalog URL.

## Regenerating `setup.slt`

```bash
bash crates/sqllogictest/dev/regen-snowplow-setup.sh
```

The script knows the DAG order and per-model output schema (per
`test-dbt-snowplow-web/dbt_project.yml`: `scratch/*` → `_scratch`,
`*/manifest/*` → `_snowplow_manifest`, everything else → `_derived`) and
emits both Phase A and Phase B sections from the same dbt-compiled SQL
sources.

For the 4 incremental-materialised models, the script extracts the verbatim
MERGE block from
`test-dbt-snowplow-web/target/run/snowplow_web/models/.../<model>.sql`. That
file only exists after dbt has been invoked against a live embucket: a
`dbt run --select <model>` writes the wrapped MERGE to `target/run/` before
attempting execution (it's fine if execution then fails — the wrapping is
already on disk). To regenerate it from scratch:

```bash
# In ../test-dbt-snowplow-web, against a running embucketd:
.venv/bin/dbt run --select snowplow_web_sessions snowplow_web_page_views \
                           snowplow_web_users snowplow_web_user_mapping
```

If a model's `target/run/.../<model>.sql` is missing or doesn't contain
`merge into`, `regen-snowplow-setup.sh` aborts with an actionable error.

## Regenerating the parquet fixtures

The parquet files are committed copies of the upstream `snowplow-events-parquet`
pipeline output. To refresh (replace `<runs-dir>` with the absolute path to
that project's `runs/` directory):

```bash
cp <runs-dir>/<run1>/parquet/data.parquet \
   crates/sqllogictest/tests/fixtures/snowplow/events1.parquet
cp <runs-dir>/<run2>/parquet/data.parquet \
   crates/sqllogictest/tests/fixtures/snowplow/events2.parquet
```

Each file is ~300 KB / 200 rows / 133 typed columns. The two runs should
cover overlapping wall-clock windows with disjoint event_ids and shared
sessions/users — that's the pattern Phase B's MERGE is designed to
exercise. Bound constants in the dbt-compiled SQL (and thus in `setup.slt`
after regen) must enclose both files' `collector_tstamp` ranges.

`load_tstamp` is baked at upstream `now()` time; once the parquet files
are committed it's stable across test runs (no per-run drift).

## Regenerating the leaf `.slt` files

The 36 query `.slt` files under `tests/slt/dbt_snowplow_web/` are mechanical
wrappers around verbatim dbt-compiled SQL from the sibling `test-dbt-snowplow-web`
project's `queries/` directory (set `DBT_QUERIES_DIR` to override the default
path in the regen script). To rebuild after the upstream compiler output
changes:

```bash
bash crates/sqllogictest/dev/regen-snowplow-slt.sh
```
