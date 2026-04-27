use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSnafu, Result};
use iceberg_file_catalog::FileCatalogList;
use iceberg_rest_catalog::apis::configuration::Configuration;
use iceberg_rest_catalog::catalog::RestCatalogList;
use iceberg_rust::catalog::CatalogList;
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust::object_store::ObjectStoreBuilder;
use object_store::local::LocalFileSystem;
use snafu::ResultExt;
use std::sync::Arc;

/// Build a catalog list rooted at `catalog_url`. The URL scheme selects the
/// catalog implementation and the object store backend:
/// - `http:` / `https:` → Iceberg REST catalog at the given base path
/// - `s3:` → `iceberg-file-catalog` over S3 (env-configured)
/// - `file:` → `iceberg-file-catalog` over the local filesystem
/// - anything else → `iceberg-file-catalog` over in-memory storage (used by tests)
///
/// Registers a single default catalog under [`DEFAULT_CATALOG`].
pub async fn build_dev_catalog_list(
    config: CatalogListConfig,
    catalog_url: &str,
) -> Result<Arc<EmbucketCatalogList>> {
    let catalog_list: Arc<dyn CatalogList> =
        if catalog_url.starts_with("http:") || catalog_url.starts_with("https:") {
            let configuration = Configuration {
                base_path: catalog_url.to_string(),
                ..Default::default()
            };
            Arc::new(RestCatalogList::new(
                configuration,
                Some(ObjectStoreBuilder::s3()),
                false,
            ))
        } else {
            let object_store = if catalog_url.starts_with("s3:") {
                ObjectStoreBuilder::s3()
            } else if catalog_url.starts_with("file:") {
                ObjectStoreBuilder::Filesystem(Arc::new(LocalFileSystem::new()))
            } else {
                ObjectStoreBuilder::memory()
            };
            Arc::new(
                FileCatalogList::new(catalog_url, object_store)
                    .await
                    .map_err(IcebergError::from)
                    .context(IcebergSnafu)?,
            )
        };

    let embucket = Arc::new(EmbucketCatalogList::new(config));

    if let Some(catalog) = catalog_list.catalog(DEFAULT_CATALOG) {
        embucket
            .register_iceberg_catalog(DEFAULT_CATALOG, catalog, false)
            .await?;
    }

    Ok(embucket)
}
