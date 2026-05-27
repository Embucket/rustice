# Snowplow fixtures

Fixture data and setup SQL for the `dbt_snowplow_web` sqllogictest suite
(`crates/sqllogictest/tests/slt/dbt_snowplow_web/`).

## Files

- **`events.csv`** — First 200 rows of the canonical Snowplow enriched-events
  TSV at `snowplow-events-parquet/runs/20260429T184310Z/tsv/enriched/enriched_0001`.
  No header row. 131 columns. Tab-delimited (despite the `.csv` extension —
  the COPY INTO sets `FIELD_DELIMITER = '\t'`). Empty string encodes NULL.
  Named `.csv` because `ListingOptions::with_file_extension` defaults to
  `.csv` for `CsvFormat`; a `.tsv` filename is silently filtered out.
- **`setup.header.slt`** — Hand-maintained bootstrap: schemas, the
  `enriched_raw` staging table + `COPY INTO` of `events.csv`, the typed
  `events` CTAS (mirroring `snowplow-events-parquet/sql/tsv_to_parquet.sql.tmpl`),
  and empty stubs for the three dim seeds (`snowplow_web_dim_*`). Column shapes
  for the dim stubs match the headers of the dbt package's seed CSVs at
  `dbt_packages/snowplow_web/seeds/*.csv`.
- **`setup.slt`** — Generated file (do not edit by hand). Concatenates
  `setup.header.slt` with a per-model materialisation chain built from the dbt
  compiled SQL. For each of the 18 models, in DAG order:
    1. `CREATE TABLE <schema>.<model> AS <full-refresh SELECT>` — lays down the
       schema; the full-refresh `9999-01-01` sentinel timestamps mean the table
       is typically empty after this step.
    2. Step 2 branches on the model's canonical dbt materialisation:
       - **`+materialized: incremental`** (the 4 derived models: sessions,
         page_views, users, user_mapping): materialise a `<model>__dbt_tmp`
         source from the incremental SELECT, then run the **verbatim MERGE
         INTO** statement that dbt-snowflake's incremental materialisation
         writes to `target/run/.../<model>.sql`. Full enumerated column lists
         and unique_key predicates as produced by dbt.
       - **`+materialized: table`** (everything else): `INSERT INTO <schema>.<model>
         <incremental SELECT>`.
  This mirrors the production dbt flow on a cold warehouse: full-refresh once,
  then incremental thereafter — including the MERGE upsert for incremental
  models.

## How it's wired in

Leaf `.slt` files pull setup in with a plain relative include:

```
include ../../../fixtures/snowplow/setup.slt
```

Upstream `sqllogictest::parse_file` resolves relative include paths against
the including file's directory.

The `COPY INTO` in `setup.header.slt` references the TSV through the
upstream variable-substitution mechanism:

```
control substitution on

statement ok
COPY INTO embucket.public_snowplow_manifest_scratch.enriched_raw
FROM 'file://${CRATE_ROOT}/tests/fixtures/snowplow/events.csv'
FILE_FORMAT = ( TYPE = 'CSV' FIELD_DELIMITER = '\\t' SKIP_HEADER = 0 );

control substitution off
```

The harness publishes `CRATE_ROOT` to the runner via `Runner::set_var`
(value: `env!("CARGO_MANIFEST_DIR")`), so the committed `.slt` doesn't bake
in machine-specific paths. Substitution is bracketed because the upstream
`subst` parser treats `\` as an escape — outside the bracket, dbt-compiled
SQL contains literal `$` references that must not be substituted; inside,
the literal tab delimiter has to be written `\\t` so `subst` produces
`\t` for the engine to consume.

For `dbt_snowplow_web/` paths the harness builds a per-file `tempfile::tempdir()`
and passes `file://<tempdir>` as the Iceberg catalog URL.

## Regenerating `setup.slt`

```bash
bash crates/sqllogictest/dev/regen-snowplow-setup.sh
```

The script knows the DAG order and per-model output schema (per
`test-dbt-snowplow-web/dbt_project.yml`: `scratch/*` → `_scratch`,
`*/manifest/*` → `_snowplow_manifest`, everything else → `_derived`).

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

## Regenerating the TSV slice

```bash
head -200 \
  /home/work/workspace/github/snowplow-events-parquet/runs/20260429T184310Z/tsv/enriched/enriched_0001 \
  > crates/sqllogictest/tests/fixtures/snowplow/events.csv
```

200 rows ≈ 600 KB. Increase only if a model needs more than ~10 distinct
sessions to exercise a code path.

## Regenerating the leaf `.slt` files

The 36 query `.slt` files under `tests/slt/dbt_snowplow_web/` are mechanical
wrappers around verbatim dbt-compiled SQL at
`/home/work/workspace/github/test-dbt-snowplow-web/queries/`. To rebuild after
the upstream compiler output changes:

```bash
bash crates/sqllogictest/dev/regen-snowplow-slt.sh
```
