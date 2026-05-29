# Snowflake Horizon Iceberg Compatibility

This file records live compatibility checks for Rustice running in Snowpark
Container Services against Snowflake-managed Iceberg tables through the Horizon
REST catalog.

The current verified scope is read compatibility in both directions for
copy-on-write Snowflake-managed Iceberg snapshots. Merge-on-read and positional
delete files were not enabled for these runs.

## Latest Verified Run

Run date: 2026-05-29

Rustice image:

```text
iwuwgvk-lv71752.registry.snowflakecomputing.com/rustice_app/public/rustice_repo/rustice:spcs-20260529-b8603be
```

Rustice commit:

```text
b8603be
```

`iceberg-rust` rev:

```text
211bd611e53628eb26de1ff9f5f31901c5cd7d60
```

SPCS service:

```text
RUSTICE_APP.PUBLIC.RUSTICE_SERVICE
```

Latest observed ingress:

```text
enxz2e-iwuwgvk-lv71752.snowflakecomputing.app
```

The service was dropped and the compute pool was suspended after the run.

## Current Compatibility Matrix

| Direction | Operation | Status | Evidence |
| --- | --- | --- | --- |
| Snowflake writes, Rustice reads | Baseline table read | Pass | `RUSTICE_SPCS.PUBLIC.SMOKE` returned `1, ok` through both Snowflake CLI and `embucket-snow` |
| Snowflake writes, Rustice reads | `INSERT` | Pass | 10,010-row aggregate matched Snowflake |
| Snowflake writes, Rustice reads | `DELETE` | Pass | 9,000-row aggregate matched Snowflake |
| Snowflake writes, Rustice reads | `UPDATE` | Pass | Updated row values matched Snowflake |
| Snowflake writes, Rustice reads | `MERGE` | Pass | Matched update plus insert aggregate matched Snowflake |
| Snowflake writes, Rustice reads | Combined `INSERT`, `DELETE`, `UPDATE`, `MERGE` | Pass | Latest compact run returned `3,1,4,23,23.2500` through both engines |
| Rustice writes, Snowflake reads | `DOUBLE`, `STRING`, `BOOLEAN` insert | Pass | Snowflake read `3,92001.0,92003.0,276006.0,2` |
| Rustice writes, Snowflake reads | Non-numeric mixed insert | Pass | Snowflake read `3,k1,k3,2,3,2024-01-01,03:04:05,2024-01-03T03:04:05` |
| Rustice writes, Snowflake reads | Temporal insert | Pass | Snowflake read `2,1.0,2.0,2024-01-01,02:03:04,2024-01-02T02:03:04` |
| Rustice writes, Snowflake reads | Binary insert | Pass | Snowflake read `2,1.0,2.0,2` |
| Rustice writes, Snowflake reads | `NUMBER(38,0)` insert | Pass | Snowflake read `2,1,4,5` |
| Rustice writes, Snowflake reads | `NUMBER(18,4)` insert | Pass | Snowflake read `2,3.2500,6.5000,9.7500` |
| Rustice writes, Snowflake reads | `NUMBER(5,0)` insert | Pass | Latest SPCS run: Rustice inserted `(2), (5)` and Snowflake read `2`, `5` |
| Rustice writes, Snowflake reads | Wide all-types insert | Pass | Rustice-created and Snowflake-created wide tables both read back in Snowflake with matching aggregates |
| Rustice writes, Snowflake reads | Simple Rustice `MERGE` | Pass | Rustice updated one row and inserted one row; Snowflake read the same result |
| Rustice writes, Snowflake reads | Standalone Rustice `UPDATE` | Fail | Returned success but did not change table rows |
| Rustice writes, Snowflake reads | Rustice `DELETE` | Fail | `DELETE not supported for Base table` |
| Mixed Snowflake/Rustice writers | Snowflake-created table | Fail | Rustice `INSERT` after Snowflake `INSERT` replaced the table with only Rustice rows |
| Mixed Snowflake/Rustice writers | Rustice-created table | Fail | Snowflake `INSERT` was visible to Snowflake but Rustice read a stale snapshot and the next Rustice write failed with `409 Conflict` |
| External writer controls | PyIceberg simple append | Pass | Snowflake read `3,90001.0,90003.0,270006.0,2` |
| External writer controls | Spark Iceberg simple insert | Pass | Snowflake read `3,91001.0,91003.0,273006.0,2` |

## Latest Retest Details

The latest run used this deployment shape:

```bash
SNOW_CONFIG_FILE=/home/artem/.snowflake/config.toml \
SNOW_CONNECTION=snowflake \
RUSTICE_HORIZON_DATABASE=RUSTICE_SPCS \
RUSTICE_HORIZON_ROLE=RUSTICE_SPCS_ROLE \
RUSTICE_GRANT_TO_ROLE=ACCOUNTADMIN \
RUSTICE_CLIENT_DATABASE=rustice_spcs \
RUSTICE_CLIENT_SCHEMA=public \
RUSTICE_BUILD_LOCAL=1 \
RUSTICE_IMAGE_TAG=spcs-20260529-b8603be \
RUSTICE_HORIZON_SCHEMAS=PUBLIC,public \
RUSTICE_HORIZON_TABLES=PUBLIC.SMOKE \
RUSTICE_INSTANCE_FAMILY=CPU_X64_XS \
RUSTICE_POOL_MIN_NODES=1 \
RUSTICE_POOL_MAX_NODES=1 \
RUSTICE_MIN_INSTANCES=1 \
RUSTICE_MAX_INSTANCES=1 \
RUSTICE_AUTO_SUSPEND_SECS=0 \
RUSTICE_READY_TIMEOUT_SECS=900 \
./deploy/spcs/deploy.sh
```

The first start failed because `ICEBERG_REST_TABLES=PUBLIC.SMOKE` was configured
before `RUSTICE_SPCS.PUBLIC.SMOKE` existed. After creating that Snowflake-managed
Iceberg table and redeploying, the service reached `READY`.

Smoke check:

| Engine | Query | Result |
| --- | --- | --- |
| Snowflake | `SELECT * FROM RUSTICE_SPCS.PUBLIC.SMOKE` | `1, ok` |
| Rustice through SPCS | `SELECT * FROM rustice_spcs.public.smoke` | `1, ok` |

Small decimal check:

```sql
CREATE TABLE rustice_spcs.public.compat_number5_spcs_retest (
  small_num NUMBER(5,0)
);
INSERT INTO rustice_spcs.public.compat_number5_spcs_retest VALUES (2), (5);
```

| Engine | Query | Result |
| --- | --- | --- |
| Rustice through SPCS | `SELECT small_num FROM rustice_spcs.public.compat_number5_spcs_retest ORDER BY small_num` | `2`, `5` |
| Snowflake | `SELECT "small_num" FROM RUSTICE_SPCS.PUBLIC."compat_number5_spcs_retest" ORDER BY "small_num"` | `2`, `5` |

Mixed Snowflake-write check:

Snowflake created `RUSTICE_SPCS.PUBLIC."sf_ops_spcs_retest"`, inserted three
rows, deleted one row, updated one row, and ran one `MERGE` with a matched update
and an insert. Rustice read the new snapshot through SPCS after the table was
included in `RUSTICE_HORIZON_TABLES`.

Aggregate result:

| Engine | `count,min,max,sum_small,sum_dec` |
| --- | --- |
| Snowflake | `3,1,4,23,23.2500` |
| Rustice through SPCS | `3,1,4,23,23.2500` |

Row-level result:

| Engine | Rows |
| --- | --- |
| Snowflake | `1,-1,-1.2500,false,one-merged,2024-01-11,2024-01-11 11:00:00`; `2,20,20.5000,false,two-updated,2024-01-02,2024-01-02 02:00:00`; `4,4,4.0000,true,four,2024-01-04,2024-01-04 04:00:00` |
| Rustice through SPCS | `1,-1,-1.2500,False,one-merged,2024-01-11,2024-01-11 11:00:00`; `2,20,20.5000,False,two-updated,2024-01-02,2024-01-02 02:00:00`; `4,4,4.0000,True,four,2024-01-04,2024-01-04 04:00:00` |

Boolean display casing differs between CLIs, but the values match.

Full reverse-table retest:

Two wide reverse-write variants were rerun after the small-decimal metadata fix.
Both tables used the same all-types shape as the larger Snowflake-write model:
`NUMBER(38,0)`, `NUMBER(5,0)`, `NUMBER(18,4)`, `DOUBLE`, `FLOAT`, `BOOLEAN`,
`STRING`, `VARCHAR`, `BINARY`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, and
`TIMESTAMP_LTZ`.

| Table | Created by | Written by | Rustice result | Snowflake result | Status |
| --- | --- | --- | --- | --- | --- |
| `compat_rustice_all_types_retest` | Rustice | Rustice `INSERT` | `10000,1,10000,50005000,62506250.0000,5000,10000` | Same | Pass |
| `compat_reverse_sf_all_types_retest` | Snowflake | Rustice `INSERT` | `10000,1,10000,50005000,62506250.0000,5000,10000` | Same | Pass |

Result tuple order:

```text
row_count,min_id,max_id,sum_small,sum_dec,true_count,bin_count
```

Sample rows checked in Snowflake for both tables:

| `id` | `small_num` | `dec_col` | `dbl_col` | `float_col` | `bool_col` | `str_col` | `varchar_col` | `bin_col` | `date_col` | `time_col` | `ts_ntz` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `1` | `1` | `1.2500` | `0.3333` | `0.5` | `false` | `str-1` | `varchar-1` | `abcd` | `2022-01-01` | `00:00:00` | `2022-08-21T00:00:00` |
| `42` | `42` | `52.5000` | `14.0` | `21.0` | `true` | `str-42` | `varchar-42` | `abcd` | `2022-01-01` | `00:00:00` | `2022-08-21T00:00:00` |
| `10000` | `10000` | `12500.0000` | `3333.3333` | `5000.0` | `true` | `str-10000` | `varchar-10000` | `abcd` | `2022-01-01` | `00:00:00` | `2022-08-21T00:00:00` |

`TIMESTAMP_LTZ` display depends on Snowflake session timezone. The same inserted
instant was visible in Snowflake as `2022-08-21T07:00:00` for the
Rustice-created table and `2022-08-20T23:00:00-08:00` for the Snowflake-created
table. This did not affect aggregate or row-value compatibility for the tested
non-timezone columns.

## Mixed Writer Retest

Run date: 2026-05-29

Mixed writer tables were tested to check whether Snowflake and Rustice can write
to the same managed Iceberg table in alternating order. This is not compatible
yet.

### Snowflake-Created Table

Table:

```text
RUSTICE_SPCS.PUBLIC."mixed_sf_created_retest"
```

Sequence:

| Step | Writer | Operation | Observed result |
| --- | --- | --- | --- |
| 1 | Snowflake | Insert `id` `1,2` | Snowflake read `1,2` |
| 2 | Rustice | Insert `id` `3,4` | Rustice read only `3,4`; Snowflake also read only `3,4` |
| 3 | Snowflake | Insert `id` `5` | Snowflake read `3,4,5` |
| 4 | Rustice | Read table | Rustice still read stale `3,4` |
| 5 | Rustice | Insert `id` `6` | Failed with Horizon `409 Conflict` because branch `main` changed |

The first Rustice write after the Snowflake write lost the existing Snowflake
rows. After Snowflake wrote again, Rustice did not refresh to the new snapshot
and the next Rustice write failed with a commit conflict instead of silently
overwriting.

Conflict message:

```text
Requirement failed: branch main has changed
```

### Rustice-Created Table

Table:

```text
RUSTICE_SPCS.PUBLIC."mixed_rustice_created_retest"
```

Sequence:

| Step | Writer | Operation | Observed result |
| --- | --- | --- | --- |
| 1 | Rustice | Create table and insert `id` `1,2` | Rustice and Snowflake read `1,2` |
| 2 | Snowflake | Insert `id` `3` | Snowflake read `1,2,3` |
| 3 | Rustice | Read table | Rustice still read stale `1,2` |
| 4 | Rustice | Insert `id` `4` | Failed with Horizon `409 Conflict` because branch `main` changed |

The Rustice-created path avoids silent row loss in this sequence, but Rustice
still serves stale data after the Snowflake write and cannot continue writing
without refreshing or reloading table metadata.

## Rustice DML Retest

Run date: 2026-05-29

Rustice DML was checked on a clean Rustice-created table without concurrent
Snowflake writes:

```text
RUSTICE_SPCS.PUBLIC."rustice_ops_clean_retest"
```

Results:

| Operation | Rustice result | Snowflake result | Status |
| --- | --- | --- | --- |
| `INSERT` rows `1,2,3` | Rows visible | Same | Pass |
| Standalone `UPDATE id = 1` | Statement returned success, but rows stayed unchanged | Same unchanged rows | Fail |
| `DELETE id = 2` | Failed: `DELETE not supported for Base table` | No delete committed | Fail |
| `MERGE` update `id = 1`, insert `id = 4` | `1` row updated and `1` row inserted | Snowflake read `1,111,111.0000,one-merged`; `2,2,2.0000,two`; `3,3,3.0000,three`; `4,4,4.0000,four-merged` | Pass |

This means Rustice can commit a simple Iceberg `MERGE` result that Snowflake can
read, but standalone `UPDATE` and `DELETE` are not currently reliable for
Iceberg tables.

## Snowflake-Write Test Model

The larger Snowflake-write run used five independent tables, each starting from
the same 10,000-row dataset:

| Table | Purpose |
| --- | --- |
| `compat_insert_only` | Snowflake `INSERT`, then Rustice read |
| `compat_delete_only` | Snowflake `DELETE`, then Rustice read |
| `compat_update_only` | Snowflake `UPDATE`, then Rustice read |
| `compat_merge_only` | Snowflake `MERGE`, then Rustice read |
| `compat_combined` | `INSERT`, `DELETE`, `UPDATE`, `MERGE` in sequence, then Rustice read |

Table shape:

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

Baseline aggregate for each table:

```text
10000,1,10000,50005000,62506250.0000,5000
```

Result tuple order:

```text
row_count,min_id,max_id,sum_small,sum_dec,true_count
```

Snowflake-write results:

| Test | Snowflake write | Snowflake result | Rustice result | Status |
| --- | --- | --- | --- | --- |
| Insert-only | Insert `id` `10001..10010` | `10010,1,10010,50105055,62631318.7500,5005` | Same | Pass |
| Delete-only | `DELETE WHERE MOD(id, 10) = 0` | `9000,1,9999,45000000,56250000.0000,4000` | Same | Pass |
| Update-only | Update `id = 42` | Row values matched | Row values matched | Pass |
| Merge-only | Matched update for `id = 1` plus insert for `id = 20001` | `10001,1,20001,50025001,62526249.0000,5001` | Same | Pass |
| Combined | `INSERT`, `DELETE`, `UPDATE`, `MERGE` | `9010,1,20001,45110046,56382460.7500,4004` | Same | Pass |

## Type Coverage

Observed Snowflake-managed Iceberg DDL limits:

| Type probe | Result |
| --- | --- |
| `NUMBER`, `DOUBLE`, `FLOAT`, `BOOLEAN`, `STRING`, unbounded `VARCHAR`, `BINARY`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ` | Supported |
| `VARCHAR(100)` | Rejected by Snowflake-managed Iceberg; unbounded `VARCHAR` works |
| `TIMESTAMP_TZ` | Rejected |
| `ARRAY`, `VARIANT` | Rejected |

## Historical Investigation

Before the small-decimal fix in `iceberg-rust`, Rustice writes containing
`NUMBER(5,0)` were visible to Rustice but Snowflake returned zero rows. This was
isolated to generated Iceberg data-file statistics for small-precision decimals,
not to Horizon commit visibility in general. The full reverse-table retest above
confirms that the wide table now works after the fix.

Historical failing table:

| Test table | Types | Rustice result | Snowflake result | Status before fix |
| --- | --- | --- | --- | --- |
| `compat_rustice_number5_probe` | `NUMBER(5,0)` | `2,2,5,7` | `0,,,` | Fail |
| `compat_rustice_decimal_probe` | `NUMBER(38,0)`, `NUMBER(5,0)`, `NUMBER(18,4)` | `2` rows | `0` rows | Fail |

The data-file lower/upper bounds for field `1` looked like:

```text
lower {1: b'\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x02\\x00\\x00\\x00'}
upper {1: b'\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x05\\x00\\x00\\x00'}
```

Those bytes were not Iceberg decimal's minimal big-endian two's-complement
encoding for values `2` and `5`. The fix landed in `iceberg-rust#58` and is
included in the current Rustice dependency rev through `iceberg-rust#59`.

Other Rustice write probes already passed before the small-decimal fix:

| Test table | Types | Snowflake result | Status |
| --- | --- | --- | --- |
| `compat_rustice_append_simple` | `DOUBLE`, `STRING`, `BOOLEAN` | `3,92001.0,92003.0,276006.0,2` | Pass |
| `compat_rustice_no_numeric_probe` | `STRING`, `BOOLEAN`, `BINARY`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ` | `3,k1,k3,2,3,2024-01-01,03:04:05,2024-01-03T03:04:05` | Pass |
| `compat_rustice_temporal_probe` | `DOUBLE`, `DATE`, `TIME`, `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ` | `2,1.0,2.0,2024-01-01,02:03:04,2024-01-02T02:03:04` | Pass |
| `compat_rustice_binary_probe` | `DOUBLE`, `BINARY` | `2,1.0,2.0,2` | Pass |
| `compat_rustice_number38_probe` | `NUMBER(38,0)` | `2,1,4,5` | Pass |
| `compat_rustice_decimal18_probe` | `NUMBER(18,4)` | `2,3.2500,6.5000,9.7500` | Pass |

`ALTER ICEBERG TABLE ... REFRESH` did not help with the historical failure:
Snowflake reported that `REFRESH` requires an external catalog integration and
that the table type is `MANAGED`.

`SYSTEM$GET_ICEBERG_TABLE_INFORMATION` returned the managed metadata location,
but Snowflake still read zero rows for the old failing table.

## External Writer Controls

The same Snowflake account and Horizon REST endpoint were checked with external
writers that bypass Rustice entirely:

| Test | Writer | Table | Snowflake result | Status |
| --- | --- | --- | --- | --- |
| Simple append | PyIceberg `0.9.1` | `compat_pyiceberg_append_simple` | `3,90001.0,90003.0,270006.0,2` | Pass |
| Simple insert | Spark `3.5.1` + Iceberg runtime `1.9.1` | `compat_spark_append_simple` | `3,91001.0,91003.0,273006.0,2` | Pass |

PyIceberg also reached the file-writing path for a wider table containing
`decimal(18,4)`, but failed locally before commit while collecting Parquet
statistics:

```text
Unexpected physical type FIXED_LEN_BYTE_ARRAY for decimal(18, 4), expected INT64
```

That PyIceberg-specific decimal failure is separate from the Rustice write-path
issue. The simple PyIceberg and Spark controls confirm that Snowflake can read
successful external writes committed through Horizon REST for Snowflake-managed
Iceberg tables.

## Operational Notes

- With `ICEBERG_REST_EAGER_LOAD=0`, existing Snowflake-created tables that need
  to be visible immediately must be listed in `RUSTICE_HORIZON_TABLES`.
- Bootstrap tables listed in `RUSTICE_HORIZON_TABLES` must exist before service
  startup. Otherwise Rustice fails fast during catalog initialization.
- Rustice-created table names are visible in Snowflake under the Snowflake
  catalog namespace. In the latest run, `rustice_spcs.public.<table>` was
  visible to Snowflake as `RUSTICE_SPCS.PUBLIC."<table>"`.
- Timestamp and boolean string formatting can differ between Snowflake CLI and
  `embucket-snow`; row values were compared semantically.

## Remaining Gaps

- Fix mixed Snowflake/Rustice writer support by refreshing table metadata before
  reads and before commits, and by handling Horizon `409 Conflict` with a clear
  retry/error path.
- Fix standalone Rustice `UPDATE` on Iceberg tables; it currently reports
  success without changing data.
- Implement or explicitly reject Rustice `DELETE` for Iceberg tables; it
  currently fails with `DELETE not supported for Base table`.
- Retest Rustice `MERGE` on Snowflake-created tables and wider schemas. The
  clean Rustice-created simple-table `MERGE` path passes.
- Test `ICEBERG_MERGE_ON_READ_BEHAVIOR = enabled` and positional delete files
  separately.
- Add automated coverage for the parts that can run without live Snowflake SPCS
  credentials. Live SPCS/Horizon compatibility remains a manual integration
  check for now.
