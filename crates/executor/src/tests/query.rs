use crate::session::UserSession;
use std::collections::HashMap;

use crate::models::QueryContext;
use crate::running_queries::RunningQueriesRegistry;
use crate::service::CoreExecutionService;
use crate::utils::Config;
use datafusion::sql::parser::DFParser;
use functions::session_params::SessionProperty;
use std::sync::Arc;

// TODO: This test is disabled because it requires metastore bootstrapping.
// The test needs to be updated to work with the Iceberg REST Catalog.
#[allow(clippy::unwrap_used, clippy::large_futures)]
#[tokio::test]
#[ignore = "requires Iceberg REST Catalog setup - to be fixed in follow-up"]
async fn test_update_all_table_names_visitor() {
    let args = vec![
        ("select * from foo", "SELECT * FROM embucket.new_schema.foo"),
        (
            "insert into foo (id) values (5)",
            "INSERT INTO embucket.new_schema.foo (id) VALUES (5)",
        ),
        (
            "insert into foo select * from bar",
            "INSERT INTO embucket.new_schema.foo SELECT * FROM embucket.new_schema.bar",
        ),
        (
            "insert into foo select * from bar where id = 1",
            "INSERT INTO embucket.new_schema.foo SELECT * FROM embucket.new_schema.bar WHERE id = 1",
        ),
        (
            "select * from foo join bar on foo.id = bar.id",
            "SELECT * FROM embucket.new_schema.foo JOIN embucket.new_schema.bar ON foo.id = bar.id",
        ),
        (
            "select * from foo where id = 1",
            "SELECT * FROM embucket.new_schema.foo WHERE id = 1",
        ),
        (
            "select count(*) from foo",
            "SELECT count(*) FROM embucket.new_schema.foo",
        ),
        (
            "WITH sales_data AS (SELECT * FROM foo) SELECT * FROM sales_data",
            "WITH sales_data AS (SELECT * FROM embucket.new_schema.foo) SELECT * FROM sales_data",
        ),
        (
            "SELECT * from flatten('[1,77]','',false,false,'both')",
            "SELECT * FROM flatten('[1,77]', '', false, false, 'both')",
        ),
    ];

    let session = create_df_session().await;
    let mut params = HashMap::new();
    params.insert(
        "schema".to_string(),
        SessionProperty::from_str_value("schema".to_string(), "new_schema".to_string(), None),
    );
    session.set_session_variable(true, params).await.unwrap();
    let query = session.query("", QueryContext::default());
    for (init, exp) in args {
        let statement = DFParser::parse_sql(init).unwrap().pop_front();
        if let Some(mut s) = statement {
            query.update_statement_references(&mut s).unwrap();
            assert_eq!(s.to_string(), exp);
        }
    }
}

const TABLE_SETUP: &str = include_str!(r"./table_setup.sql");

#[allow(clippy::unwrap_used, clippy::expect_used)]
pub async fn create_df_session() -> Arc<UserSession> {
    let running_queries = Arc::new(RunningQueriesRegistry::new());
    let config = Arc::new(Config::default());
    let catalog_list = CoreExecutionService::catalog_list(&config)
        .expect("Failed to create catalog list");
    let runtime_env = CoreExecutionService::runtime_env(&config, catalog_list.clone())
        .expect("Failed to create runtime env");

    let user_session = Arc::new(
        UserSession::new(
            running_queries,
            Arc::new(Config::default()),
            catalog_list,
            runtime_env,
            "",
        )
        .await
        .expect("Failed to create user session"),
    );

    for q in TABLE_SETUP.split(';') {
        let q = q.trim();
        if q.is_empty() {
            continue;
        }
        let mut query = user_session.query(q, QueryContext::default());
        // These will fail without a real catalog, but that's expected in the current state
        let _ = query.execute().await;
    }
    user_session
}
