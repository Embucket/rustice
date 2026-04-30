use crate::test_query;

// Smoke test for COPY INTO against the s3:// FileCatalog backend. Ignored by
// default because it requires AWS credentials and a real bucket. Set
// COPY_INTO_S3_CATALOG_URL (e.g. s3://my-bucket/warehouse) and AWS env vars,
// then run with `cargo test -- --ignored copy_into_file_catalog_s3`.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::large_futures)]
#[tokio::test]
#[ignore = "requires AWS credentials and COPY_INTO_S3_CATALOG_URL env var"]
async fn copy_into_file_catalog_s3() {
    use crate::models::QueryContext;
    use crate::tests::query::create_df_session_with_catalog_url;

    let catalog_url = std::env::var("COPY_INTO_S3_CATALOG_URL").expect("COPY_INTO_S3_CATALOG_URL");
    let source_url = std::env::var("COPY_INTO_S3_SOURCE_URL").expect("COPY_INTO_S3_SOURCE_URL");

    let session = create_df_session_with_catalog_url(&catalog_url).await;

    let mut q = session.query(
        "CREATE TABLE embucket.public.copy_into_s3 (id INT, name VARCHAR);",
        QueryContext::default(),
    );
    q.execute().await.expect("create table");

    let copy_sql = format!(
        "COPY INTO embucket.public.copy_into_s3 FROM '{source_url}' FILE_FORMAT = ( TYPE = 'CSV' );"
    );
    let mut q = session.query(&copy_sql, QueryContext::default());
    q.execute().await.expect("copy into");

    let mut q = session.query(
        "SELECT COUNT(*) FROM embucket.public.copy_into_s3;",
        QueryContext::default(),
    );
    let res = q.execute().await.expect("select count");
    assert!(!res.records.is_empty());
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::large_futures)]
#[tokio::test]
async fn copy_into_file_catalog_local() {
    use crate::models::QueryContext;
    use crate::tests::query::create_df_session_with_catalog_url;

    let tempdir = tempfile::tempdir().expect("tempdir");
    let csv_path = tempdir.path().join("source.csv");
    std::fs::write(&csv_path, "1,alice\n2,bob\n3,carol\n").expect("write csv");

    let catalog_url = format!("file://{}", tempdir.path().display());
    let session = create_df_session_with_catalog_url(&catalog_url).await;

    let mut q = session.query(
        "CREATE TABLE embucket.public.copy_into_local (id INT, name VARCHAR);",
        QueryContext::default(),
    );
    q.execute().await.expect("create table");

    let copy_sql = format!(
        "COPY INTO embucket.public.copy_into_local FROM 'file://{}' FILE_FORMAT = ( TYPE = 'CSV' );",
        csv_path.display()
    );
    let mut q = session.query(&copy_sql, QueryContext::default());
    q.execute().await.expect("copy into");

    let mut q = session.query(
        "SELECT COUNT(*) FROM embucket.public.copy_into_local;",
        QueryContext::default(),
    );
    let res = q.execute().await.expect("select count");
    let formatted = datafusion::arrow::util::pretty::pretty_format_batches(&res.records)
        .unwrap()
        .to_string();
    assert!(
        formatted.contains("3"),
        "expected count 3, got: {formatted}",
    );
}

test_query!(
    create_table_with_timestamps,
    "SELECT * FROM timestamps",
   setup_queries = [
        "CREATE TABLE timestamps (
            ntz TIMESTAMP_NTZ, ntz_0 TIMESTAMP_NTZ(0), ntz_3 TIMESTAMP_NTZ(3), ntz_6 TIMESTAMP_NTZ(6), ntz_9 TIMESTAMP_NTZ(9),
            ltz TIMESTAMP_LTZ, ltz_0 TIMESTAMP_LTZ(0), ltz_3 TIMESTAMP_LTZ(3), ltz_6 TIMESTAMP_LTZ(6), ltz_9 TIMESTAMP_LTZ(9),
            tz TIMESTAMP_TZ, tz_0 TIMESTAMP_TZ(0), tz_3 TIMESTAMP_TZ(3), tz_6 TIMESTAMP_TZ(6), tz_9 TIMESTAMP_TZ(9),
            dt DATETIME, dt_0 DATETIME(0), dt_3 DATETIME(3), dt_6 DATETIME(6), dt_9 DATETIME(9))
        as SELECT * FROM VALUES (
            '2025-04-09T21:11:23','2025-04-09T22:11:23','2025-04-09T23:11:23','2025-04-09T20:11:23','2025-04-09T19:11:23',
            '2025-04-09T21:11:23','2025-04-09T22:11:23','2025-04-09T23:11:23','2025-04-09T20:11:23','2025-04-09T19:11:23',
            '2025-04-09T21:11:23','2025-04-09T22:11:23','2025-04-09T23:11:23','2025-04-09T20:11:23','2025-04-09T19:11:23',
            '2025-04-09T21:11:23','2025-04-09T22:11:23','2025-04-09T23:11:23','2025-04-09T20:11:23','2025-04-09T19:11:23'
        );"
    ],
    snapshot_path = "table"
);

test_query!(
    create_table_and_insert,
    "SELECT * FROM embucket.public.test",
    setup_queries = [
        "CREATE TABLE embucket.public.test (id INT)",
        "INSERT INTO embucket.public.test VALUES (1), (2)",
    ],
    snapshot_path = "table"
);

test_query!(
    create_table_as_select,
    "SELECT * FROM embucket.public.testtable",
    setup_queries = [
        "CREATE OR REPLACE TABLE embucket.public.testtable AS SELECT NULL AS DEFAULT",
        "INSERT INTO embucket.public.testtable VALUES (null), ('fff')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_table_as_select_from_values,
    "SELECT * FROM embucket.public.testtable",
    setup_queries = [
        "CREATE OR REPLACE TABLE embucket.public.testtable AS SELECT * FROM VALUES (null)",
        "INSERT INTO embucket.public.testtable VALUES (null), ('fff')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_table_quoted_identifiers,
    "SELECT * FROM embucket.\"test public\".\"test table\"",
    setup_queries = [
        "CREATE SCHEMA embucket.\"test public\"",
        "CREATE TABLE embucket.\"test public\".\"test table\" (id INT)",
        "INSERT INTO embucket.\"test public\".\"test table\" VALUES (1), (2)",
    ],
    snapshot_path = "table"
);

// CREATE TABLE with casting timestamp nanosecond to iceberg timestamp microseconds
test_query!(
    create_table_with_casting_timestamp,
    "CREATE OR REPLACE TABLE t1 AS
        SELECT * FROM (VALUES ('2021-03-02 15:55:18.539000'::TIMESTAMP)) AS t(start_tstamp);",
    snapshot_path = "table"
);

test_query!(
    drop_table,
    "SHOW TABLES IN public STARTS WITH 'test'",
    setup_queries = [
        "CREATE TABLE embucket.public.test (id INT) as VALUES (1), (2)",
        "DROP TABLE embucket.public.test"
    ],
    snapshot_path = "table"
);

test_query!(
    drop_table_quoted_identifiers,
    "SHOW TABLES IN public STARTS WITH 'test'",
    setup_queries = [
        "CREATE SCHEMA embucket.\"test public\"",
        "CREATE TABLE embucket.\"test public\".\"test table\" (id INT)",
        "INSERT INTO embucket.\"test public\".\"test table\" VALUES (1), (2)",
        "DROP TABLE embucket.\"test public\".\"test table\"",
    ],
    snapshot_path = "table"
);

test_query!(
    drop_table_missing_schema,
    "DROP TABLE embucket.missing.table",
    snapshot_path = "table"
);

test_query!(
    drop_table_missing,
    "DROP TABLE embucket.public.missing",
    snapshot_path = "table"
);

test_query!(
    alter_table,
    "ALTER TABLE embucket.public.test ADD COLUMN new_col INT",
    setup_queries = ["CREATE TABLE embucket.public.test (id INT) as VALUES (1), (2)",],
    snapshot_path = "table"
);

test_query!(
    alter_iceberg_table,
    "ALTER ICEBERG TABLE test ADD col INT;",
    setup_queries = ["CREATE TABLE embucket.public.test (id INT) as VALUES (1), (2)",],
    snapshot_path = "table"
);

test_query!(
    alter_missing_schema,
    "ALTER TABLE embucket.missing.table ADD COLUMN new_col INT",
    snapshot_path = "table"
);

test_query!(
    alter_missing_table,
    "ALTER TABLE embucket.public.missing ADD COLUMN new_col INT",
    snapshot_path = "table"
);

test_query!(
    alter_table_stub_should_pass,
    "ALTER TABLE embucket.test.some_table add column c5 VARCHAR",
    setup_queries = [
        "CREATE SCHEMA embucket.test",
        "CREATE TABLE embucket.test.some_table (id INT)",
    ],
    snapshot_path = "table"
);

test_query!(
    alter_table_if_exists_stub_should_pass,
    "ALTER TABLE IF EXISTS embucket.test.some_table add column c5 VARCHAR",
    setup_queries = [
        "CREATE SCHEMA embucket.test",
        "CREATE TABLE embucket.test.some_table (id INT)",
    ],
    snapshot_path = "table"
);

test_query!(
    alter_table_missing_catalog_snowflake_error,
    "ALTER TABLE missing_catalog.public.some_table add column c5 VARCHAR",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    alter_table_if_exists_missing_catalog_snowflake_error,
    "ALTER TABLE IF EXISTS missing_catalog.public.some_table add column c5 VARCHAR",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    alter_table_missing_schema_snowflake_error,
    "ALTER TABLE embucket.missing_schema.some_table add column c5 VARCHAR",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    alter_table_if_exists_missing_schema_snowflake_error,
    "ALTER TABLE IF EXISTS embucket.missing_schema.some_table add column c5 VARCHAR",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    alter_table_missing_table_snowflake_error,
    "ALTER TABLE embucket.public.missing_table add column c5 VARCHAR",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    alter_table_if_exists_missing_table_should_pass,
    "ALTER TABLE IF EXISTS embucket.public.missing_table add column c5 VARCHAR",
    snapshot_path = "table"
);

test_query!(
    drop_table_missing_catalog_snowflake_error,
    "DROP TABLE missing_catalog.public.some_table",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    drop_table_if_exists_missing_catalog_snowflake_error,
    "DROP TABLE IF EXISTS missing_catalog.public.some_table",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    drop_table_missing_schema_snowflake_error,
    "DROP TABLE embucket.missing_schema.some_table",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    drop_table_if_exists_missing_schema_snowflake_error,
    "DROP TABLE IF EXISTS embucket.missing_schema.some_table",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    drop_table_missing_table_snowflake_error,
    "DROP TABLE embucket.public.missing",
    snapshot_path = "snowflake_error",
    snowflake_error = true
);

test_query!(
    drop_table_if_exists_missing_table_should_pass,
    "DROP TABLE IF EXISTS embucket.public.missing",
    snapshot_path = "table"
);

test_query!(
    drop_table_stub_should_pass,
    "DROP TABLE embucket.test.some_table",
    setup_queries = [
        "CREATE SCHEMA embucket.test",
        "CREATE TABLE embucket.test.some_table (id INT)",
    ],
    snapshot_path = "table"
);

test_query!(
    drop_table_if_exists_uppercase_quoted_table_name,
    "DROP TABLE IF EXISTS \"EMBUCKET\".\"PUBLIC_SNOWPLOW_MANIFEST\".\"TABLE_TO_DROP\" cascade",
    setup_queries = [
        "CREATE SCHEMA embucket.public_snowplow_manifest",
        "CREATE TABLE embucket.public_snowplow_manifest.table_to_drop (id INT)",
    ],
    snapshot_path = "table"
);

// TRUNCATE TABLE
test_query!(
    truncate_table,
    "TRUNCATE TABLE employee_table",
    snapshot_path = "table"
);
test_query!(
    truncate_table_full,
    "SELECT count() FROM embucket.public.employee_table",
    setup_queries = ["TRUNCATE TABLE embucket.public.employee_table"],
    snapshot_path = "table"
);
test_query!(
    truncate_table_full_quotes,
    "TRUNCATE TABLE 'EMBUCKET'.'PUBLIC'.'EMPLOYEE_TABLE'",
    snapshot_path = "table"
);
test_query!(
    truncate_missing,
    "TRUNCATE TABLE missing_table",
    snapshot_path = "table"
);

// CREATE ICEBERG TABLE with PARTITION BY — Snowflake-style ICEBERG TABLE
// statements with iceberg partitioning specs.
test_query!(
    create_iceberg_table_partition_by_single_column,
    "SELECT * FROM embucket.public.parted_single ORDER BY id",
    setup_queries = [
        "CREATE ICEBERG TABLE embucket.public.parted_single (id INT, name VARCHAR)
            EXTERNAL_VOLUME = 'test_vol'
            CATALOG = 'SNOWFLAKE'
            BASE_LOCATION = '/data/parted_single'
            PARTITION BY (id)",
        "INSERT INTO embucket.public.parted_single VALUES (1, 'alice'), (2, 'bob')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_multiple_columns,
    "SELECT * FROM embucket.public.parted_multi ORDER BY id",
    setup_queries = [
        "CREATE ICEBERG TABLE embucket.public.parted_multi (id INT, region VARCHAR, name VARCHAR)
            EXTERNAL_VOLUME = 'test_vol'
            CATALOG = 'SNOWFLAKE'
            BASE_LOCATION = '/data/parted_multi'
            PARTITION BY (region, id)",
        "INSERT INTO embucket.public.parted_multi VALUES (1, 'us', 'alice'), (2, 'eu', 'bob')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_year_transform,
    "SELECT count(*) FROM embucket.public.parted_year",
    setup_queries = [
        "CREATE ICEBERG TABLE embucket.public.parted_year (id INT, ts TIMESTAMP)
            EXTERNAL_VOLUME = 'test_vol'
            CATALOG = 'SNOWFLAKE'
            BASE_LOCATION = '/data/parted_year'
            PARTITION BY (year(ts))",
        "INSERT INTO embucket.public.parted_year VALUES
            (1, '2024-03-15 10:00:00'::TIMESTAMP),
            (2, '2025-07-22 14:30:00'::TIMESTAMP)",
    ],
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_bucket_transform,
    "SELECT count(*) FROM embucket.public.parted_bucket",
    setup_queries = [
        "CREATE ICEBERG TABLE embucket.public.parted_bucket (id INT, name VARCHAR)
            EXTERNAL_VOLUME = 'test_vol'
            CATALOG = 'SNOWFLAKE'
            BASE_LOCATION = '/data/parted_bucket'
            PARTITION BY (bucket(16, id))",
        "INSERT INTO embucket.public.parted_bucket VALUES (1, 'alice'), (2, 'bob'), (3, 'carol')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_truncate_transform,
    "SELECT count(*) FROM embucket.public.parted_truncate",
    setup_queries = [
        "CREATE ICEBERG TABLE embucket.public.parted_truncate (id INT, name VARCHAR)
            EXTERNAL_VOLUME = 'test_vol'
            CATALOG = 'SNOWFLAKE'
            BASE_LOCATION = '/data/parted_truncate'
            PARTITION BY (truncate(100, id))",
        "INSERT INTO embucket.public.parted_truncate VALUES (1, 'alice'), (250, 'bob')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_mixed_transforms,
    "SELECT count(*) FROM embucket.public.parted_mixed",
    setup_queries = [
        "CREATE ICEBERG TABLE embucket.public.parted_mixed (id INT, ts TIMESTAMP, name VARCHAR)
            EXTERNAL_VOLUME = 'test_vol'
            CATALOG = 'SNOWFLAKE'
            BASE_LOCATION = '/data/parted_mixed'
            PARTITION BY (year(ts), bucket(8, id))",
        "INSERT INTO embucket.public.parted_mixed VALUES
            (1, '2024-03-15 10:00:00'::TIMESTAMP, 'alice'),
            (25, '2025-07-22 14:30:00'::TIMESTAMP, 'bob')",
    ],
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_unknown_column,
    "CREATE ICEBERG TABLE embucket.public.parted_bad_col (id INT, name VARCHAR)
        EXTERNAL_VOLUME = 'test_vol'
        CATALOG = 'SNOWFLAKE'
        BASE_LOCATION = '/data/parted_bad_col'
        PARTITION BY (does_not_exist)",
    snapshot_path = "table"
);

test_query!(
    create_iceberg_table_partition_by_unknown_transform,
    "CREATE ICEBERG TABLE embucket.public.parted_bad_xform (id INT)
        EXTERNAL_VOLUME = 'test_vol'
        CATALOG = 'SNOWFLAKE'
        BASE_LOCATION = '/data/parted_bad_xform'
        PARTITION BY (foo(id))",
    snapshot_path = "table"
);

// Reads from a public S3 testdata bucket; exercises COPY INTO end-to-end against
// the default memory-backed dev FileCatalog. Ignored by default because it
// requires network access to AWS S3 — run with `cargo test -- --ignored` against
// a host with outbound HTTPS to s3.amazonaws.com.
test_query!(
    copy_into_without_volume,
    "SELECT SUM(L_QUANTITY) FROM embucket.public.lineitem;",
    setup_queries = [
        "CREATE TABLE embucket.public.lineitem (
    L_ORDERKEY BIGINT NOT NULL,
    L_PARTKEY BIGINT NOT NULL,
    L_SUPPKEY BIGINT NOT NULL,
    L_LINENUMBER INT NOT NULL,
    L_QUANTITY DOUBLE NOT NULL,
    L_EXTENDED_PRICE DOUBLE NOT NULL,
    L_DISCOUNT DOUBLE NOT NULL,
    L_TAX DOUBLE NOT NULL,
    L_RETURNFLAG CHAR NOT NULL,
    L_LINESTATUS CHAR NOT NULL,
    L_SHIPDATE DATE NOT NULL,
    L_COMMITDATE DATE NOT NULL,
    L_RECEIPTDATE DATE NOT NULL,
    L_SHIPINSTRUCT VARCHAR NOT NULL,
    L_SHIPMODE VARCHAR NOT NULL,
    L_COMMENT VARCHAR NOT NULL );",
        "COPY INTO embucket.public.lineitem FROM 's3://embucket-testdata/tpch/lineitem.csv' FILE_FORMAT = ( TYPE = CSV );"
    ],
    snapshot_path = "table",
    ignore_reason = "requires network access to s3.amazonaws.com"
);
