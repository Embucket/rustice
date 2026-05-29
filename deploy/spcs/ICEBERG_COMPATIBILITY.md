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

## External Writer Control Checks

To separate Snowflake/Horizon behavior from Rustice write behavior, the same
account and Horizon REST endpoint were checked with external writers that bypass
Rustice entirely.

Connection details:

| Setting | Value |
| --- | --- |
| Horizon URI | `https://IWUWGVK-LV71752.snowflakecomputing.com/polaris/api/catalog` |
| Warehouse / database | `RUSTICE_SPCS` |
| Role scope | `session:role:ACCOUNTADMIN` |
| Auth | Short-lived PAT for `RUSTICE_HORIZON_SVC`, removed after each test |

Results:

| Test | Writer | Table | Snowflake result | Status |
| --- | --- | --- | --- | --- |
| Simple append | PyIceberg `0.9.1` | `compat_pyiceberg_append_simple` | `3,90001.0,90003.0,270006.0,2` | Pass |
| Simple insert | Spark `3.5.1` + Iceberg runtime `1.9.1` | `compat_spark_append_simple` | `3,91001.0,91003.0,273006.0,2` | Pass |

Result tuple order:
`row_count,min_id,max_id,sum_id,true_count`.

Snowflake row checks:

| Table | Rows |
| --- | --- |
| `compat_pyiceberg_append_simple` | `90001.0,pyiceberg-a,True`; `90002.0,pyiceberg-b,False`; `90003.0,pyiceberg-c,True` |
| `compat_spark_append_simple` | `91001.0,spark-a,True`; `91002.0,spark-b,False`; `91003.0,spark-c,True` |

PyIceberg also reached the file-writing path for a wider table containing
`decimal(18,4)`, but failed locally before commit while collecting Parquet
statistics:

```text
Unexpected physical type FIXED_LEN_BYTE_ARRAY for decimal(18, 4), expected INT64
```

That decimal-specific PyIceberg failure is separate from the Rustice reverse
write issue. The simple PyIceberg and Spark positive controls confirm that
Snowflake can see successful external writes committed through Horizon REST for
Snowflake-managed Iceberg tables.

## Rustice Type Isolation Checks

The reverse Rustice-write issue is not a generic write/commit failure. After
the external-writer controls, smaller Rustice writes were run through SPCS to
isolate the failing type family.

| Test table | Types | Rustice result | Snowflake result | Status |
| --- | --- | --- | --- | --- |
| `compat_rustice_append_simple` | `DOUBLE`, `STRING`, `BOOLEAN` | `3,92001.0,92003.0,276006.0,2` | Same | Pass |
| `compat_rustice_no_numeric_probe` | `STRING`, `BOOLEAN`, `BINARY`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ` | `3,k1,k3,2,3,2024-01-01,03:04:05,2024-01-03T03:04:05` | Same | Pass |
| `compat_rustice_temporal_probe` | `DOUBLE`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ` | `2,1.0,2.0,2024-01-01,02:03:04,2024-01-02T02:03:04` | Same | Pass |
| `compat_rustice_binary_probe` | `DOUBLE`, `BINARY` | `2,1.0,2.0,2` | Same | Pass |
| `compat_rustice_number38_probe` | `NUMBER(38,0)` | `2,1,4,5` | Same | Pass |
| `compat_rustice_number5_probe` | `NUMBER(5,0)` | `2,2,5,7` | `0,,,` | Fail |
| `compat_rustice_decimal18_probe` | `NUMBER(18,4)` | `2,3.2500,6.5000,9.7500` | Same | Pass |
| `compat_rustice_decimal_probe` | `NUMBER(38,0)`, `NUMBER(5,0)`, `NUMBER(18,4)` | `2` rows | `0` rows | Fail |

The failing wide reverse table includes `small_num NUMBER(5,0)`, matching the
isolated failing probe. This points to Rustice's Iceberg metadata/statistics
generation for small-precision decimal values rather than Horizon commit
visibility in general.

For `compat_rustice_number5_probe`, the manifest data-file lower/upper bounds
for field `1` were:

```text
lower {1: b'\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x02\\x00\\x00\\x00'}
upper {1: b'\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x05\\x00\\x00\\x00'}
```

Those bounds do not look like Iceberg decimal's minimal big-endian two's
complement byte representation for values `2` and `5`. A likely fix is in
`iceberg-rust`'s Parquet-to-DataFile statistics conversion path for decimal
types, especially small-precision decimals that Arrow/Parquet stores as `INT32`.

## Small Decimal Retest

Run date: 2026-05-28

After the `iceberg-rust` fix in
[`Embucket/iceberg-rust#58`](https://github.com/Embucket/iceberg-rust/pull/58),
Rustice was rebuilt and deployed to SPCS as:

```text
iwuwgvk-lv71752.registry.snowflakecomputing.com/rustice_app/public/rustice_repo/rustice:iceberg-decimal-fix-20260529
```

The Rustice dependency rev used for this image was:

```text
f3481358ec2e4abadb1533b1dbe11644c4e07e74
```

Retest:

| Test table | Types | Rustice result | Snowflake result | Status |
| --- | --- | --- | --- | --- |
| `compat_rustice_number5_fix_embucket` | `DECIMAL(5,0)` | `2`, `5` | `2`, `5` | Pass |

Rustice query:

```sql
CREATE TABLE rustice_spcs.public.compat_rustice_number5_fix_embucket (
  small_num DECIMAL(5,0)
);
INSERT INTO rustice_spcs.public.compat_rustice_number5_fix_embucket VALUES (2), (5);
SELECT small_num
FROM rustice_spcs.public.compat_rustice_number5_fix_embucket
ORDER BY small_num;
```

Snowflake query:

```sql
SELECT "small_num"
FROM RUSTICE_SPCS."public"."compat_rustice_number5_fix_embucket"
ORDER BY "small_num";
```

Snowflake returned the same two rows: `2`, `5`.

Reverse-direction conclusion: the isolated small-precision decimal failure is
fixed by `iceberg-rust#58`. Wider Rustice write compatibility and Rustice
`MERGE` behavior still need to be rerun after Rustice consumes that
`iceberg-rust` commit.

## Current SPCS Retest

Run date: 2026-05-29

Rustice was rebuilt from `b8603be` and deployed to SPCS as:

```text
iwuwgvk-lv71752.registry.snowflakecomputing.com/rustice_app/public/rustice_repo/rustice:spcs-20260529-b8603be
```

The Rustice dependency rev for `iceberg-rust` was:

```text
211bd611e53628eb26de1ff9f5f31901c5cd7d60
```

SPCS service state:

```text
RUSTICE_APP.PUBLIC.RUSTICE_SERVICE: READY
ingress: mmxz2e-iwuwgvk-lv71752.snowflakecomputing.app
```

The first deploy attempt failed because `ICEBERG_REST_TABLES=PUBLIC.SMOKE` was
configured before `RUSTICE_SPCS.PUBLIC.SMOKE` existed. After creating the
Snowflake-managed Iceberg smoke table and redeploying, the service started
successfully.

| Test | Snowflake result | Rustice result | Status |
| --- | --- | --- | --- |
| Snowflake-created `PUBLIC.SMOKE` baseline | `1, ok` | `1, ok` | Pass |
| Rustice-created `compat_number5_spcs_retest` with `NUMBER(5,0)` | `2`, `5` | `2`, `5` | Pass |
| Snowflake-created `sf_ops_spcs_retest` after `INSERT`, `DELETE`, `UPDATE`, `MERGE` | `3,1,4,23,23.2500` | `3,1,4,23,23.2500` | Pass |

`sf_ops_spcs_retest` used a compact row-level check:

| Engine | Rows |
| --- | --- |
| Snowflake | `1,-1,-1.2500,false,one-merged,2024-01-11,2024-01-11 11:00:00`; `2,20,20.5000,false,two-updated,2024-01-02,2024-01-02 02:00:00`; `4,4,4.0000,true,four,2024-01-04,2024-01-04 04:00:00` |
| Rustice | `1,-1,-1.2500,False,one-merged,2024-01-11,2024-01-11 11:00:00`; `2,20,20.5000,False,two-updated,2024-01-02,2024-01-02 02:00:00`; `4,4,4.0000,True,four,2024-01-04,2024-01-04 04:00:00` |

Because the SPCS service runs with `ICEBERG_REST_EAGER_LOAD=0`, existing
Snowflake-created tables that must be visible immediately need to be listed in
`RUSTICE_HORIZON_TABLES`. This retest used:

```text
PUBLIC.SMOKE,PUBLIC.sf_ops_spcs_retest,PUBLIC.compat_number5_spcs_retest
```

The retest confirms that the current image can read Snowflake-written snapshots
after mixed write operations and that the small-precision decimal metadata fix
is visible to Snowflake. Rustice `MERGE` planning remains a separate follow-up.

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
