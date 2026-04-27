use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSnafu, Result};
use iceberg_file_catalog::FileCatalogList;
use iceberg_rust::catalog::CatalogList;
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust::object_store::ObjectStoreBuilder;
use snafu::ResultExt;
use std::sync::Arc;

/// Build an in-memory dev-mode catalog list backed by an in-memory
/// `iceberg-file-catalog` and an in-memory object store.
///
/// Registers a single default catalog under the name [`DEFAULT_CATALOG`].
pub async fn build_dev_catalog_list(config: CatalogListConfig) -> Result<Arc<EmbucketCatalogList>> {
    let object_store = ObjectStoreBuilder::memory();
    let file_catalog_list = Arc::new(
        FileCatalogList::new("/dev", object_store)
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
