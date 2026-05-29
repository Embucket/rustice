# snowplow_ddl

Focused DDL tests for the operations the dbt-snowplow-web setup
(`tests/fixtures/snowplow/setup.slt`) relies on. Each file exercises one
operation in isolation so that when the multi-stage snowplow harness
misbehaves we can point at the exact DDL primitive that's broken.

Coverage:

| File | Operation under test |
|------|----------------------|
| `create_schema_idempotent.slt` | `CREATE SCHEMA IF NOT EXISTS` runs twice without error |
| `create_or_replace_table.slt`  | `CREATE OR REPLACE TABLE` (typed cols and `AS SELECT`) actually drops & recreates |
| `create_table_as_then_insert.slt` | Snowplow Phase A pattern: CTAS (possibly empty) + INSERT INTO — no duplication |
| `drop_then_create.slt` | Snowplow Phase B pattern: DROP + fresh CREATE TABLE AS |
| `copy_into_parquet.slt` | COPY INTO from a parquet fixture |
| `merge_upsert.slt` | MERGE INTO with WHEN MATCHED / WHEN NOT MATCHED — no row duplication |
