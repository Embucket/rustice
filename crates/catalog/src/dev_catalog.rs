use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSqlCatalogSnafu, Result};
use iceberg_rust::catalog::CatalogList;
use iceberg_rust::object_store::ObjectStoreBuilder;
use iceberg_sql_catalog::SqlCatalogList;
use snafu::ResultExt;
use std::sync::Arc;

/// Build an in-memory dev-mode catalog list backed by an in-memory SQLite
/// Iceberg SQL catalog and an in-memory object store.
///
/// Registers a single default catalog under the name [`DEFAULT_CATALOG`].
pub async fn build_dev_catalog_list(
    config: CatalogListConfig,
) -> Result<Arc<EmbucketCatalogList>> {
    let object_store = ObjectStoreBuilder::memory();
    let sql_catalog_list = Arc::new(
        SqlCatalogList::new("sqlite://", object_store)
            .await
            .context(IcebergSqlCatalogSnafu)?,
    );

    let embucket = Arc::new(EmbucketCatalogList::new(config));

    if let Some(catalog) = sql_catalog_list.catalog(DEFAULT_CATALOG) {
        embucket.register_iceberg_catalog(DEFAULT_CATALOG, catalog, false);
    }

    Ok(embucket)
}
