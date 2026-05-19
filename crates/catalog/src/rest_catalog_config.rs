use crate::error::{IcebergSnafu, Result};
use iceberg_rest_catalog::apis::{
    configuration::{Configuration, OAuthAccessTokenProvider},
    o_auth2_api_api,
};
use iceberg_rust::error::Error as IcebergError;
use snafu::ResultExt;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const ICEBERG_REST_PREFIX_ENV: &str = "ICEBERG_REST_PREFIX";
const ICEBERG_REST_BEARER_TOKEN_ENV: &str = "ICEBERG_REST_BEARER_TOKEN";
const ICEBERG_REST_OAUTH_TOKEN_ENV: &str = "ICEBERG_REST_OAUTH_TOKEN";
const ICEBERG_REST_CREDENTIAL_ENV: &str = "ICEBERG_REST_CREDENTIAL";
const ICEBERG_REST_SCOPE_ENV: &str = "ICEBERG_REST_SCOPE";
const ICEBERG_REST_ROLE_ENV: &str = "ICEBERG_REST_ROLE";
const ICEBERG_REST_CLIENT_ID_ENV: &str = "ICEBERG_REST_CLIENT_ID";
const ICEBERG_REST_EAGER_LOAD_ENV: &str = "ICEBERG_REST_EAGER_LOAD";
const ICEBERG_REST_SCHEMAS_ENV: &str = "ICEBERG_REST_SCHEMAS";
const ICEBERG_REST_TABLES_ENV: &str = "ICEBERG_REST_TABLES";
const DEFAULT_TOKEN_TTL_SECS: u64 = 300;
const TOKEN_REFRESH_PERCENT: u64 = 70;

#[derive(Clone)]
struct CachedOAuthToken {
    token: String,
    refresh_before: SystemTime,
}

pub fn rest_catalog_prefix(default_catalog: &str) -> String {
    env_non_empty(ICEBERG_REST_PREFIX_ENV).unwrap_or_else(|| default_catalog.into())
}

pub fn rest_catalog_eager_load() -> bool {
    matches!(
        env_non_empty(ICEBERG_REST_EAGER_LOAD_ENV)
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

pub fn rest_catalog_bootstrap_schemas() -> Vec<String> {
    env_non_empty(ICEBERG_REST_SCHEMAS_ENV)
        .map(|schemas| {
            schemas
                .split(',')
                .map(str::trim)
                .filter(|schema| !schema.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .filter(|schemas: &Vec<String>| !schemas.is_empty())
        .unwrap_or_else(|| vec!["PUBLIC".to_string(), "public".to_string()])
}

pub fn rest_catalog_bootstrap_tables() -> Vec<String> {
    env_non_empty(ICEBERG_REST_TABLES_ENV)
        .map(|tables| {
            tables
                .split(',')
                .map(str::trim)
                .filter(|table| !table.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub async fn configure_rest_catalog_auth(configuration: &mut Configuration) -> Result<()> {
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
    configuration.oauth_access_token = Some(
        refreshing_oauth_token_provider(configuration, credential, scope, client_id)
            .await
            .context(IcebergSnafu)?,
    );
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
    })
}

async fn refreshing_oauth_token_provider(
    configuration: &Configuration,
    credential: String,
    scope: String,
    client_id: Option<String>,
) -> std::result::Result<OAuthAccessTokenProvider, IcebergError> {
    let mut token_configuration = configuration.clone();
    token_configuration.bearer_access_token = None;
    token_configuration.oauth_access_token = None;

    let initial_token = fetch_oauth_token(
        &token_configuration,
        &credential,
        &scope,
        client_id.as_deref(),
    )
    .await?;
    let cached_token = Arc::new(Mutex::new(Some(initial_token)));
    let token_configuration = Arc::new(token_configuration);
    let credential = Arc::new(credential);
    let scope = Arc::new(scope);
    let client_id = Arc::new(client_id);

    Ok(Arc::new(move || {
        let cached_token = Arc::clone(&cached_token);
        let token_configuration = Arc::clone(&token_configuration);
        let credential = Arc::clone(&credential);
        let scope = Arc::clone(&scope);
        let client_id = Arc::clone(&client_id);

        Box::pin(async move {
            {
                let cached = cached_token.lock().map_err(|error| {
                    IcebergError::InvalidFormat(format!(
                        "Horizon token cache lock poisoned: {error}"
                    ))
                })?;
                if let Some(token) = cached.as_ref()
                    && token.refresh_before > SystemTime::now()
                {
                    return Ok(token.token.clone());
                }
            }

            let fresh_token = fetch_oauth_token(
                &token_configuration,
                &credential,
                &scope,
                client_id.as_ref().as_deref(),
            )
            .await?;
            let token = fresh_token.token.clone();
            let mut cached = cached_token.lock().map_err(|error| {
                IcebergError::InvalidFormat(format!("Horizon token cache lock poisoned: {error}"))
            })?;
            *cached = Some(fresh_token);
            Ok(token)
        })
    }))
}

async fn fetch_oauth_token(
    configuration: &Configuration,
    credential: &str,
    scope: &str,
    client_id: Option<&str>,
) -> std::result::Result<CachedOAuthToken, IcebergError> {
    let token = o_auth2_api_api::get_token(
        configuration,
        Some("client_credentials"),
        Some(scope),
        client_id,
        Some(credential),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .map_err(|error| IcebergError::External(Box::new(error)))?;

    let ttl_secs = token
        .expires_in
        .and_then(|expires_in| u64::try_from(expires_in).ok())
        .filter(|expires_in| *expires_in > 0)
        .unwrap_or(DEFAULT_TOKEN_TTL_SECS);
    let refresh_after_secs = ttl_secs
        .saturating_mul(TOKEN_REFRESH_PERCENT)
        .saturating_div(100)
        .max(1);
    let refresh_before = SystemTime::now()
        .checked_add(Duration::from_secs(refresh_after_secs))
        .unwrap_or_else(SystemTime::now);

    Ok(CachedOAuthToken {
        token: token.access_token,
        refresh_before,
    })
}
