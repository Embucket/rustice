use crate::server::error::CreateExecutorSnafu;
use crate::server::error::Result;
use crate::server::server_models::RestApiConfig;
use api_snowflake_rest_sessions::session::SessionStore;
use executor::service::CoreExecutionService;
use executor::utils::Config as ExecutionConfig;
use snafu::ResultExt;
use std::sync::Arc;
use tokio::time::Duration;

pub struct CoreState {
    pub executor: Arc<CoreExecutionService>,
    pub rest_api_config: RestApiConfig,
}

impl CoreState {
    pub async fn new(
        execution_cfg: ExecutionConfig,
        rest_api_config: RestApiConfig,
    ) -> Result<Self> {
        let executor = create_executor(execution_cfg).await?;
        Ok(Self {
            executor,
            rest_api_config,
        })
    }

    pub fn with_session_timeout(&self, session_timeout: Duration) -> Result<()> {
        tracing::info!(
            "With session timeout, by {} seconds",
            session_timeout.as_secs()
        );
        let session_store = SessionStore::new(self.executor.clone());
        tokio::spawn(async move {
            session_store
                .continuously_delete_expired(session_timeout)
                .await;
        });
        Ok(())
    }
}

async fn create_executor(
    execution_cfg: ExecutionConfig,
) -> Result<Arc<CoreExecutionService>> {
    tracing::info!("Creating execution service");
    let executor = Arc::new(
        CoreExecutionService::new(Arc::new(execution_cfg))
            .await
            .context(CreateExecutorSnafu)?,
    );
    tracing::info!("Execution service created");
    Ok(executor)
}
