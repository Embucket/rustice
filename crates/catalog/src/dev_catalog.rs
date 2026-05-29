use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSnafu, Result};
use crate::rest_catalog_config::{
    configure_rest_catalog_auth, rest_catalog_access_delegation, rest_catalog_bootstrap_schemas,
    rest_catalog_bootstrap_tables, rest_catalog_eager_load, rest_catalog_prefix,
    rest_catalog_sql_catalog,
};
use datafusion::execution::object_store::ObjectStoreRegistry;
use iceberg_file_catalog::FileCatalogList;
use iceberg_rest_catalog::apis::configuration::Configuration;
use iceberg_rest_catalog::catalog::RestCatalog;
use iceberg_rust::catalog::{Catalog, CatalogList};
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust::object_store::{Bucket, ObjectStoreBuilder};
use object_store::local::LocalFileSystem;
use snafu::ResultExt;
use std::sync::Arc;
use url::Url;

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
    let embucket = Arc::new(EmbucketCatalogList::new(config));

    let catalog_list: Arc<dyn CatalogList> =
        if catalog_url.starts_with("http:") || catalog_url.starts_with("https:") {
            let mut configuration = Configuration {
                base_path: catalog_url.to_string(),
                ..Default::default()
            };
            configuration.access_delegation = rest_catalog_access_delegation();
            configure_rest_catalog_auth(&mut configuration).await?;

            let rest_prefix = rest_catalog_prefix(DEFAULT_CATALOG);
            let rest_sql_catalog = rest_catalog_sql_catalog(DEFAULT_CATALOG);
            let catalog: Arc<dyn Catalog> = Arc::new(RestCatalog::new(
                Some(&rest_prefix),
                configuration,
                Some(ObjectStoreBuilder::s3()),
                false,
            ));
            if rest_catalog_eager_load() {
                embucket
                    .register_iceberg_catalog(&rest_sql_catalog, catalog, false)
                    .await?;
            } else {
                let bootstrap_schemas = rest_catalog_bootstrap_schemas();
                let bootstrap_tables = rest_catalog_bootstrap_tables();
                embucket
                    .register_iceberg_catalog_lazy(
                        &rest_sql_catalog,
                        catalog,
                        &bootstrap_schemas,
                        &bootstrap_tables,
                        false,
                    )
                    .await?;
            }
            return Ok(embucket);
        } else {
            let object_store_builder = if catalog_url.starts_with("s3:") {
                ObjectStoreBuilder::s3()
            } else if catalog_url.starts_with("file:") {
                ObjectStoreBuilder::Filesystem(Arc::new(LocalFileSystem::new()))
            } else {
                ObjectStoreBuilder::memory()
            };
            // Make the catalog's underlying object store discoverable by DataFusion's
            // ObjectStoreRegistry under the same scheme://host key DataFusion uses,
            // so COPY INTO's Iceberg writer can resolve it. Sharing the Arc keeps
            // the FileCatalog and the writer pointed at the same client (creds,
            // region, in-memory state, etc.).
            //
            // Only register when `catalog_url` is a well-formed URL with a scheme;
            // legacy schemeless paths (used by some tests) are left to fall through
            // to the seed entries on `EmbucketCatalogList`.
            if let Ok(catalog_url_parsed) = Url::parse(catalog_url)
                && let (Ok(bucket), builder) =
                    (Bucket::from_path(catalog_url), object_store_builder.clone())
                && let Ok(catalog_object_store) = builder.build(bucket)
            {
                embucket.register_store(&catalog_url_parsed, catalog_object_store);
            }
            Arc::new(
                FileCatalogList::new(catalog_url, object_store_builder)
                    .await
                    .map_err(IcebergError::from)
                    .context(IcebergSnafu)?,
            )
        };

    if let Some(catalog) = catalog_list.catalog(DEFAULT_CATALOG) {
        embucket
            .register_iceberg_catalog(DEFAULT_CATALOG, catalog, false)
            .await?;
    }

    Ok(embucket)
}
