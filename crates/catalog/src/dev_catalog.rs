use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSnafu, Result};
use datafusion::execution::object_store::ObjectStoreRegistry;
use iceberg_file_catalog::FileCatalogList;
use iceberg_rest_catalog::apis::{
    configuration::{Configuration, OAuthAccessTokenProvider},
    o_auth2_api_api,
};
use iceberg_rest_catalog::catalog::RestCatalog;
use iceberg_rust::catalog::{Catalog, CatalogList};
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust::object_store::{Bucket, ObjectStoreBuilder};
use object_store::local::LocalFileSystem;
use snafu::ResultExt;
use std::env;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use url::Url;

const ICEBERG_REST_PREFIX_ENV: &str = "ICEBERG_REST_PREFIX";
const ICEBERG_REST_BEARER_TOKEN_ENV: &str = "ICEBERG_REST_BEARER_TOKEN";
const ICEBERG_REST_OAUTH_TOKEN_ENV: &str = "ICEBERG_REST_OAUTH_TOKEN";
const ICEBERG_REST_CREDENTIAL_ENV: &str = "ICEBERG_REST_CREDENTIAL";
const ICEBERG_REST_SCOPE_ENV: &str = "ICEBERG_REST_SCOPE";
const ICEBERG_REST_ROLE_ENV: &str = "ICEBERG_REST_ROLE";
const ICEBERG_REST_CLIENT_ID_ENV: &str = "ICEBERG_REST_CLIENT_ID";

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
            configure_rest_catalog_auth(&mut configuration).await?;

            let rest_prefix =
                env_non_empty(ICEBERG_REST_PREFIX_ENV).unwrap_or_else(|| DEFAULT_CATALOG.into());
            let catalog: Arc<dyn Catalog> = Arc::new(RestCatalog::new(
                Some(&rest_prefix),
                configuration,
                Some(ObjectStoreBuilder::s3()),
                false,
            ));
            embucket
                .register_iceberg_catalog(DEFAULT_CATALOG, catalog, false)
                .await?;
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

async fn configure_rest_catalog_auth(configuration: &mut Configuration) -> Result<()> {
    if let Some(token) = env_non_empty(ICEBERG_REST_BEARER_TOKEN_ENV) {
        configuration.bearer_access_token = Some(token);
    }

    if let Some(token) = env_non_empty(ICEBERG_REST_OAUTH_TOKEN_ENV) {
        configuration.oauth_access_token = Some(static_oauth_token_provider(token));
    }

    if configuration.bearer_access_token.is_some() || configuration.oauth_access_token.is_some() {
        return Ok(());
    }

    let Some(credential) = env_non_empty(ICEBERG_REST_CREDENTIAL_ENV) else {
        return Ok(());
    };

    let scope = env_non_empty(ICEBERG_REST_SCOPE_ENV)
        .or_else(|| env_non_empty(ICEBERG_REST_ROLE_ENV).map(|role| format!("session:role:{role}")))
        .ok_or_else(|| {
            IcebergError::InvalidFormat(format!(
                "{ICEBERG_REST_CREDENTIAL_ENV} requires {ICEBERG_REST_SCOPE_ENV} or {ICEBERG_REST_ROLE_ENV}"
            ))
        })
        .context(IcebergSnafu)?;

    let client_id = env_non_empty(ICEBERG_REST_CLIENT_ID_ENV);
    let token = o_auth2_api_api::get_token(
        configuration,
        Some("client_credentials"),
        Some(&scope),
        client_id.as_deref(),
        Some(&credential),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .map_err(|error| IcebergError::External(Box::new(error)))
    .context(IcebergSnafu)?;

    configuration.bearer_access_token = Some(token.access_token);
    Ok(())
}

fn env_non_empty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn static_oauth_token_provider(token: String) -> OAuthAccessTokenProvider {
    let token = Arc::new(token);
    Arc::new(move || {
        let token = Arc::clone(&token);
        Box::pin(async move { Ok((*token).clone()) })
            as Pin<Box<dyn Future<Output = std::result::Result<String, IcebergError>> + Send>>
    })
}
