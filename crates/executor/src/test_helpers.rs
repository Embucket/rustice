//! Session factory used by in-crate tests and by the external
//! `embucket-sqllogictest` harness.
//!
//! These helpers build a self-contained `UserSession` backed by an in-memory
//! Iceberg `FileCatalogList`, pre-create the `embucket.public` schema, and run
//! the same fixture statements that the in-crate test macro `test_query!`
//! relies on.

use crate::models::QueryContext;
use crate::running_queries::RunningQueriesRegistry;
use crate::service::CoreExecutionService;
use crate::session::UserSession;
use crate::utils::Config;
use catalog::dev_catalog::build_dev_catalog_list;
use std::sync::Arc;

const TABLE_SETUP: &str = include_str!("./tests/table_setup.sql");

#[allow(clippy::unwrap_used, clippy::expect_used)]
pub async fn create_df_session() -> Arc<UserSession> {
    create_df_session_with_catalog_url("/dev").await
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
pub async fn create_df_session_with_catalog_url(catalog_url: &str) -> Arc<UserSession> {
    let running_queries = Arc::new(RunningQueriesRegistry::new());
    let config = Arc::new(Config::default());
    let catalog_list = build_dev_catalog_list((&*config).into(), catalog_url)
        .await
        .expect("Failed to build dev catalog list");
    let runtime_env = CoreExecutionService::runtime_env(&config, catalog_list.clone())
        .expect("Failed to create runtime env");

    let user_session = Arc::new(
        UserSession::new(
            running_queries,
            config.clone(),
            catalog_list,
            runtime_env,
            "",
        )
        .await
        .expect("Failed to create user session"),
    );

    // Pre-create the default schema used by most tests.
    let mut q = user_session.query(
        "CREATE SCHEMA IF NOT EXISTS embucket.public",
        QueryContext::default(),
    );
    let _ = q.execute().await;

    for q in TABLE_SETUP.split(';') {
        let q = q.trim();
        if q.is_empty() {
            continue;
        }
        let mut query = user_session.query(q, QueryContext::default());
        let _ = query.execute().await;
    }
    user_session
}
