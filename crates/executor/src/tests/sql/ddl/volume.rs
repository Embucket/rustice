// Disabled: dev catalog (iceberg-sql-catalog) does not support
// CREATE EXTERNAL VOLUME — these statements are rejected with
// "Storage is managed by the Iceberg REST Catalog". The tests also query
// `slatedb.meta.volumes` which only exists when running against a metastore.
// Re-enable once dev mode supports volume management.
// test_query!(
//     file,
//     "SELECT volume_name, volume_type FROM slatedb.meta.volumes",
//     setup_queries = ["CREATE EXTERNAL VOLUME file STORAGE_LOCATIONS = (\
//         (NAME = 'file_vol' STORAGE_PROVIDER = 'FILE' STORAGE_BASE_URL = '/home/'))"],
//     snapshot_path = "volume"
// );

// test_query!(
//     memory,
//     "SELECT volume_name, volume_type FROM slatedb.meta.volumes",
//     setup_queries = ["CREATE EXTERNAL VOLUME mem STORAGE_LOCATIONS = (\
//         (NAME = 'mem_vol' STORAGE_PROVIDER = 'MEMORY'))"],
//     snapshot_path = "volume"
// );

// test_query!(
//     memory_if_not_exists,
//     "SELECT volume_name, volume_type FROM slatedb.meta.volumes",
//     setup_queries = [
//         "CREATE EXTERNAL VOLUME mem STORAGE_LOCATIONS = ((NAME = 'mem_vol' STORAGE_PROVIDER = 'MEMORY'))",
//         "CREATE EXTERNAL VOLUME IF NOT EXISTS mem STORAGE_LOCATIONS = ((NAME = 'mem_vol' STORAGE_PROVIDER = 'MEMORY'))",
//     ],
//     snapshot_path = "volume"
// );

// test_query!(
//     s3,
//     "SELECT volume_name, volume_type FROM slatedb.meta.volumes",
//     setup_queries = ["CREATE EXTERNAL VOLUME s3 STORAGE_LOCATIONS = ((
//             NAME = 's3-volume' STORAGE_PROVIDER = 'S3'
//             STORAGE_BASE_URL = 'bucket_name'
//             STORAGE_ENDPOINT = 'https://s3.us-east-2.amazonaws.com'
//             CREDENTIALS=(AWS_KEY_ID='1a2b3c...' AWS_SECRET_KEY='4x5y6z...' REGION='us-east-2')
//         ))"],
//     snapshot_path = "volume"
// );
