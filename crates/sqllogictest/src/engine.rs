use crate::error::{Error, Result};
use crate::normalize::{convert_batches, convert_schema_to_types};
use crate::output::{DFColumnType, DFOutput};
use async_trait::async_trait;
use executor::models::QueryContext;
use executor::session::UserSession;
use sqllogictest::DBOutput;
use std::sync::Arc;
use std::time::Duration;

/// A per-file `AsyncDB` adapter that drives rustice's `UserSession`.
pub struct EmbucketSession {
    session: Arc<UserSession>,
}

impl EmbucketSession {
    #[must_use]
    pub const fn new(session: Arc<UserSession>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl sqllogictest::AsyncDB for EmbucketSession {
    type Error = Error;
    type ColumnType = DFColumnType;

    async fn run(&mut self, sql: &str) -> Result<DFOutput> {
        let mut query = self.session.query(sql, QueryContext::default());
        let result = query
            .execute()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        let schema_ref = result.schema.as_ref();
        let types = convert_schema_to_types(schema_ref.fields());
        let rows = convert_batches(schema_ref, result.records)?;

        if rows.is_empty() && types.is_empty() {
            Ok(DBOutput::StatementComplete(0))
        } else {
            Ok(DBOutput::Rows { types, rows })
        }
    }

    async fn shutdown(&mut self) {}

    fn engine_name(&self) -> &str {
        "embucket"
    }

    async fn sleep(dur: Duration) {
        tokio::time::sleep(dur).await;
    }
}
