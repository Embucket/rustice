# Snowflake Horizon Iceberg Compatibility

This file records a manual compatibility run against Snowflake-managed Iceberg
tables through the Horizon REST catalog. The purpose is to verify whether
Rustice can read table snapshots written by Snowflake directly to Horizon,
bypassing Rustice/SPCS for writes.

Run date: 2026-05-28

## Environment

- Snowflake connection: `/home/artem/.snowflake/config.toml`, profile `snowflake`
- Rustice SPCS client config:
  `deploy/spcs/generated/config.toml`, profile `embucket_spcs`
- SPCS service: `RUSTICE_APP.PUBLIC.RUSTICE_SERVICE`
- SPCS image:
  `iwuwgvk-lv71752.registry.snowflakecomputing.com/rustice_app/public/rustice_repo/rustice:iceberg-compat-20260528`
- SPCS ingress:
  - Forward Snowflake-write run: `ilxz2e-iwuwgvk-lv71752.snowflakecomputing.app`
  - Reverse Rustice-write run: `mlxz2e-iwuwgvk-lv71752.snowflakecomputing.app`
- Horizon database/schema:
  `RUSTICE_SPCS."compat_iceberg"`
- Test mode: default Snowflake-managed Iceberg behavior. The run did not enable
  `ICEBERG_MERGE_ON_READ_BEHAVIOR`.

## Type Coverage

Each test table was created as a Snowflake-managed Iceberg table:

```sql
CREATE OR REPLACE ICEBERG TABLE RUSTICE_SPCS."compat_iceberg"."<table_name>" (
  id NUMBER(38,0),
  small_num NUMBER(5,0),
  dec_col NUMBER(18,4),
  dbl_col DOUBLE,
  float_col FLOAT,
  bool_col BOOLEAN,
  str_col STRING,
  varchar_col VARCHAR,
  bin_col BINARY,
  date_col DATE,
  time_col TIME,
  ts_ntz TIMESTAMP_NTZ,
  ts_ltz TIMESTAMP_LTZ
)
  CATALOG = 'SNOWFLAKE'
  EXTERNAL_VOLUME = 'SNOWFLAKE_MANAGED';
```

Observed Snowflake-managed Iceberg DDL limits:

| Type probe | Result |
| --- | --- |
| `NUMBER`, `DOUBLE`, `FLOAT`, `BOOLEAN`, `STRING`, unbounded `VARCHAR`, `BINARY`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ` | Supported |
| `VARCHAR(100)` | Rejected by Snowflake-managed Iceberg; unbounded `VARCHAR` works |
| `TIMESTAMP_TZ` | Rejected |
| `ARRAY`, `VARIANT` | Rejected |

## Test Tables

Five independent tables were created. Each table started with the same 10,000
rows so a failure can be attributed to one Snowflake write operation:

| Table | Purpose |
| --- | --- |
| `compat_insert_only` | Snowflake `INSERT`, then Rustice read |
| `compat_delete_only` | Snowflake `DELETE`, then Rustice read |
| `compat_update_only` | Snowflake `UPDATE`, then Rustice read |
| `compat_merge_only` | Snowflake `MERGE`, then Rustice read |
| `compat_combined` | `INSERT`, `DELETE`, `UPDATE`, `MERGE` in sequence, then Rustice read |

Base load:

```sql
INSERT INTO RUSTICE_SPCS."compat_iceberg"."<table_name>"
SELECT
  n,
  MOD(n, 99999),
  (n * 1.25)::NUMBER(18,4),
  n / 3.0,
  n * 0.5,
  MOD(n, 2) = 0,
  'str-' || n,
  'varchar-' || LPAD(n::STRING, 5, '0'),
  TO_BINARY('ABCD', 'HEX'),
  DATEADD(day, MOD(n, 365), '2022-01-01'::DATE),
  TIMEADD(second, MOD(n, 86400), '00:00:00'::TIME),
  DATEADD(second, n, '2022-08-21 00:00:00'::TIMESTAMP_NTZ),
  DATEADD(second, n, '2022-08-21 00:00:00'::TIMESTAMP_LTZ)
FROM (SELECT SEQ4() + 1 AS n FROM TABLE(GENERATOR(ROWCOUNT => 10000)));
```

Baseline Rustice read matched Snowflake for every table:

| Table | Baseline result |
| --- | --- |
| All five tables | `10000,1,10000,50005000,62506250.0000,5000` |

Result tuple order:
`row_count,min_id,max_id,sum_small,sum_dec,true_count`.

## Results

All reads below were run through `embucket-snow` against the SPCS ingress, after
Snowflake performed the write directly against Horizon. The SPCS service was not
restarted between the Snowflake write and Rustice read.

| Test | Snowflake write | Snowflake result | Rustice result | Status |
| --- | --- | --- | --- | --- |
| Insert-only | Insert `id` `10001..10010` into `compat_insert_only` | `10010,1,10010,50105055,62631318.7500,5005` | Same | Pass |
| Delete-only | `DELETE WHERE MOD(id, 10) = 0` on `compat_delete_only` | `9000,1,9999,45000000,56250000.0000,4000` | Same | Pass |
| Update-only | Update `id = 42` on `compat_update_only` | `42,42,-42.0000,False,updated-42,updated-42,2022-02-12,00:00:42,2022-08-21 00:00:42.000` | `42,42,-42.0000,False,updated-42,updated-42,2022-02-12,00:00:42,2022-08-21T00:00:42` | Pass |
| Merge-only | `MERGE` one matched update for `id = 1` and one insert for `id = 20001` on `compat_merge_only` | `10001,1,20001,50025001,62526249.0000,5001` | Same | Pass |
| Combined | `INSERT`, `DELETE`, `UPDATE`, `MERGE` in sequence on `compat_combined` | `9010,1,20001,45110046,56382460.7500,4004` | Same | Pass |

Combined scenario row-level checks:

| Engine | Rows |
| --- | --- |
| Snowflake | `1,1,-1.0000,True,merged-one,merged-one,2024-01-01,01:02:03,2024-01-01 01:02:03.000`; `42,42,-42.0000,False,updated-42,updated-42,2022-02-12,00:00:42,2022-08-21 00:00:42.000`; `20001,20001,20001.2500,False,merged-insert,merged-insert,2024-01-02,02:03:04,2024-01-02 02:03:04.000` |
| Rustice | `1,1,-1.0000,True,merged-one,merged-one,2024-01-01,01:02:03,2024-01-01T01:02:03`; `42,42,-42.0000,False,updated-42,updated-42,2022-02-12,00:00:42,2022-08-21T00:00:42`; `20001,20001,20001.2500,False,merged-insert,merged-insert,2024-01-02,02:03:04,2024-01-02T02:03:04` |

Timestamp string formatting differs between Snowflake CLI and Rustice CLI, but
the tested values match.

## Reverse Direction: Rustice Writes, Snowflake Reads

The reverse direction was also tested to check whether writes committed through
the SPCS Rustice service become visible to Snowflake-managed Iceberg tables.
This direction is not compatible yet.

Reverse test tables:

| Table | Created by | Written by | Purpose |
| --- | --- | --- | --- |
| `compat_rustice_insert_only` | Rustice | Rustice `INSERT` | Checks whether Snowflake can read a Rustice-created and Rustice-written Iceberg table |
| `compat_reverse_sf_insert` | Snowflake | Rustice `INSERT` | Checks whether Snowflake can read Rustice inserts into a Snowflake-created managed Iceberg table |
| `compat_reverse_sf_merge` | Snowflake | Rustice `MERGE` attempt | Checks whether Rustice can merge into a Snowflake-created managed Iceberg table and Snowflake can read the result |

Rustice insert statement shape:

```sql
INSERT INTO rustice_spcs.compat_iceberg.compat_reverse_sf_insert
SELECT
  value + 1,
  ((value + 1) % 99999),
  ((value + 1) * 1.25)::NUMBER(18,4),
  (value + 1) / 3.0,
  (value + 1) * 0.5,
  ((value + 1) % 2) = 0,
  'str-' || (value + 1),
  'varchar-' || (value + 1),
  TO_BINARY('ABCD', 'HEX'),
  '2022-01-01'::DATE,
  '00:00:00'::TIME,
  '2022-08-21 00:00:00'::TIMESTAMP_NTZ,
  '2022-08-21 00:00:00'::TIMESTAMP_LTZ
FROM range(10000);
```

Reverse results:

| Test | Rustice result | Snowflake result | Status |
| --- | --- | --- | --- |
| Rustice-created table, Rustice `INSERT` | `10000,1,10000,50005000,62506250.0000,5000,10000` | `0,,,,,,0` | Fail |
| Snowflake-created table, Rustice `INSERT` | `10000,1,10000,50005000,62506250.0000,5000,10000` | `0,,,,,,0` | Fail |
| Snowflake-created table, Rustice `INSERT`, then SPCS restart | `10000,1,10000,50005000,62506250.0000,5000,10000` | `0,,,,,,0` | Fail |
| Snowflake-created table, Rustice `MERGE` matched update | Planning failed: `column 'id' not found in 't'` / target field resolution failure | Not written | Fail |
| Snowflake-created table, Rustice `MERGE` insert branch with `ON FALSE` | Planning failed: `No field named rustice_spcs.compat_iceberg.compat_reverse_sf_merge.id` | Not written | Fail |

For the Rustice-created table, Snowflake also exposed the columns as quoted
lowercase identifiers (`"id"`, `"small_num"`, and so on), so unquoted Snowflake
queries failed with `invalid identifier 'ID'`. Quoted lowercase reads succeeded
syntactically, but still returned zero rows.

`ALTER ICEBERG TABLE ... REFRESH` is not applicable for these managed tables:
Snowflake reports that `REFRESH` requires an external catalog integration and
that the table type is `MANAGED`.

`SYSTEM$GET_ICEBERG_TABLE_INFORMATION` also did not make Snowflake see the
Rustice insert. Snowflake returned the original managed metadata location:

```json
{
  "metadataLocation": "s3://sfc-oh-ds1-51-customer-interop-fs-wb5f0000-s/iceberg/RUSTICE_SPCS/compat_iceberg/compat_reverse_sf_insert.40iT802O/metadata/00000-40bba4c9-ea29-4c2d-83e1-3e581c7e6457.metadata.json",
  "status": "success"
}
```

After that call, Snowflake still returned `0` rows for the table while Rustice
returned `10,000` rows after an SPCS service restart.

Reverse-direction conclusion: Rustice can currently read Snowflake-managed
Iceberg snapshots written by Snowflake, but writes performed through Rustice are
not visible to Snowflake-managed Iceberg readers. Treat Rustice writes to
Snowflake-managed Horizon tables as unsupported until the commit path is made
compatible with Snowflake's managed Iceberg metadata expectations.

## Cache Fix Verified

Before this run, the deployed image cached the Iceberg `TableProvider`, so a
Snowflake `INSERT` was invisible until the SPCS service restarted. The fix in
`crates/catalog/src/schema.rs` makes REST/Iceberg schemas bypass the
table-provider cache on lookup. The `insert-only` case above verifies that a new
Snowflake snapshot is visible without restarting SPCS.

## Out Of Scope

- `ICEBERG_MERGE_ON_READ_BEHAVIOR = enabled` and positional delete files were
  not included in this run. Treat merge-on-read/positional delete support as
  unverified.
- Automated CI coverage is still needed. This run depends on live Snowflake SPCS
  and Horizon credentials.
