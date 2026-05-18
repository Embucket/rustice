use crate::error::{IcebergSnafu, Result};
use iceberg_rest_catalog::apis::{
    configuration::{Configuration, OAuthAccessTokenProvider},
    o_auth2_api_api,
};
use iceberg_rust::error::Error as IcebergError;
use snafu::ResultExt;
use std::env;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const ICEBERG_REST_PREFIX_ENV: &str = "ICEBERG_REST_PREFIX";
const ICEBERG_REST_BEARER_TOKEN_ENV: &str = "ICEBERG_REST_BEARER_TOKEN";
const ICEBERG_REST_OAUTH_TOKEN_ENV: &str = "ICEBERG_REST_OAUTH_TOKEN";
const ICEBERG_REST_CREDENTIAL_ENV: &str = "ICEBERG_REST_CREDENTIAL";
const ICEBERG_REST_SCOPE_ENV: &str = "ICEBERG_REST_SCOPE";
const ICEBERG_REST_ROLE_ENV: &str = "ICEBERG_REST_ROLE";
const ICEBERG_REST_CLIENT_ID_ENV: &str = "ICEBERG_REST_CLIENT_ID";

pub(crate) fn rest_catalog_prefix(default_catalog: &str) -> String {
    env_non_empty(ICEBERG_REST_PREFIX_ENV).unwrap_or_else(|| default_catalog.into())
}

pub(crate) async fn configure_rest_catalog_auth(configuration: &mut Configuration) -> Result<()> {
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
