use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSnafu, Result};
use iceberg_file_catalog::FileCatalogList;
use iceberg_rust::catalog::CatalogList;
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust::object_store::ObjectStoreBuilder;
use object_store::local::LocalFileSystem;
use snafu::ResultExt;
use std::sync::Arc;

/// Build a catalog list backed by an `iceberg-file-catalog` rooted at
/// `catalog_url`. The URL scheme selects the object store backend:
/// - `s3:` → `ObjectStoreBuilder::s3()` (configured from AWS env vars)
/// - `file:` → local filesystem
/// - anything else → in-memory (used by tests)
///
/// Registers a single default catalog under [`DEFAULT_CATALOG`].
pub async fn build_dev_catalog_list(
    config: CatalogListConfig,
    catalog_url: &str,
) -> Result<Arc<EmbucketCatalogList>> {
    let object_store = if catalog_url.starts_with("s3:") {
        ObjectStoreBuilder::s3()
    } else if catalog_url.starts_with("file:") {
        ObjectStoreBuilder::Filesystem(Arc::new(LocalFileSystem::new()))
    } else {
        ObjectStoreBuilder::memory()
    };
    let file_catalog_list = Arc::new(
        FileCatalogList::new(catalog_url, object_store)
            .await
            .map_err(IcebergError::from)
            .context(IcebergSnafu)?,
    );

    let embucket = Arc::new(EmbucketCatalogList::new(config));

    if let Some(catalog) = file_catalog_list.catalog(DEFAULT_CATALOG) {
        embucket
            .register_iceberg_catalog(DEFAULT_CATALOG, catalog, false)
            .await?;
    }

    Ok(embucket)
}
