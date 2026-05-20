#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use crate::models::{
        JsonResponse, LoginRequestBody, LoginRequestData, LoginResponse, QueryRequestBody,
    };
    use crate::server::layer::require_auth;
    use crate::server::state::AppState;
    use crate::tests::create_test_server::run_test_rest_api_server;
    use crate::tests::rest_default_cfg;
    use api_snowflake_rest_sessions::TokenizedSession;
    use api_snowflake_rest_sessions::layer::Host;
    use axum::body::Bytes;
    use axum::body::{Body, to_bytes};
    use axum::extract::State;
    use axum::http;
    use axum::middleware;
    use axum::routing::post;
    use axum::{Extension, Json, Router};
    use executor::SessionMetadataAttr;
    use executor::service::make_test_execution_svc;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use reqwest::Method;
    use reqwest::header::AUTHORIZATION;
    use serde::Serialize;
    use serde_json::json;
    use std::collections::HashMap;
    use std::io::Write;
    use tower::ServiceExt;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_login() {
        let addr = run_test_rest_api_server(None, None).await;
        let client = reqwest::Client::new();
        let login_url = format!("http://127.0.0.1:{}/session/v1/login-request", addr.port());
        let query_url = format!("http://127.0.0.1:{}/queries/v1/query-request", addr.port());

        let query_request = QueryRequestBody {
            sql_text: "SELECT 1;".to_string(),
            async_exec: Some(false),
            query_submission_time: Some(1_764_161_275_445),
        };

        let query_compressed_bytes = make_bytes_body(&query_request);

        assert!(
            !query_compressed_bytes.is_empty(),
            "Compressed data should not be empty"
        );

        //Check before login without an auth header
        let res = client
            .request(
                Method::POST,
                format!("{query_url}?requestId={}", Uuid::new_v4()),
            )
            .header("Content-Type", "application/json")
            .header("Content-Encoding", "gzip")
            .body(query_compressed_bytes.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(http::StatusCode::UNAUTHORIZED, res.status());

        let login_request = LoginRequestBody {
            data: LoginRequestData {
                client_app_id: String::new(),
                client_app_version: String::new(),
                svn_revision: None,
                account_name: String::new(),
                login_name: "embucket".to_string(),
                password: "embucket".to_string(),
                client_environment: HashMap::default(),
                session_parameters: HashMap::default(),
            },
        };

        let login_compressed_bytes = make_bytes_body(&login_request);

        assert!(
            !login_compressed_bytes.is_empty(),
            "Compressed data should not be empty"
        );

        //Login
        let res = client
            .request(
                Method::POST,
                format!(
                    "{login_url}?request_id=123&databaseName=embucket&schemaName=public&warehouse=embucket"
                ),
            )
            .header("Content-Type", "application/json")
            .header("Content-Encoding", "gzip")
            .body(login_compressed_bytes)
            .send()
            .await
            .unwrap();
        assert_eq!(http::StatusCode::OK, res.status());
        let login_response: LoginResponse = res.json().await.unwrap();
        assert!(login_response.data.is_some());
        assert!(login_response.success);
        assert!(login_response.message.is_some());

        //Check after login without an auth header
        let res = client
            .request(
                Method::POST,
                format!("{query_url}?requestId={}", Uuid::new_v4()),
            )
            .header("Content-Type", "application/json")
            .header("Content-Encoding", "gzip")
            .body(query_compressed_bytes.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(http::StatusCode::UNAUTHORIZED, res.status());

        //Check after login with an auth header
        let res = client
            .request(
                Method::POST,
                format!("{query_url}?requestId={}", Uuid::new_v4()),
            )
            .header(
                AUTHORIZATION,
                format!("Snowflake Token=\"{}\"", login_response.data.unwrap().token),
            )
            .header("Content-Type", "application/json")
            .header("Content-Encoding", "gzip")
            .body(query_compressed_bytes.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(http::StatusCode::OK, res.status());
        let query_response: JsonResponse = res.json().await.unwrap();
        assert!(query_response.data.is_some());
        assert!(query_response.success);
        assert!(query_response.message.is_some());
        assert!(query_response.code.is_none()); // no code set on success
    }

    #[tokio::test]
    async fn test_spcs_trusted_ingress_login_skips_demo_password() {
        let rest_cfg = rest_default_cfg("json").with_trust_spcs_ingress(true);
        let addr = run_test_rest_api_server(Some(rest_cfg), None).await;
        let client = reqwest::Client::new();
        let login_url = format!("http://127.0.0.1:{}/session/v1/login-request", addr.port());

        let login_request = LoginRequestBody {
            data: LoginRequestData {
                client_app_id: String::new(),
                client_app_version: String::new(),
                svn_revision: None,
                account_name: String::new(),
                login_name: "not_embucket".to_string(),
                password: "not_embucket".to_string(),
                client_environment: HashMap::default(),
                session_parameters: HashMap::default(),
            },
        };

        let res = client
            .request(
                Method::POST,
                format!(
                    "{login_url}?request_id=123&databaseName=embucket&schemaName=public&warehouse=embucket"
                ),
            )
            .header("Content-Type", "application/json")
            .header("Sf-Context-Current-User", "SNOWFLAKE_USER")
            .body(serde_json::to_string(&login_request).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(http::StatusCode::OK, res.status());
        let login_response: LoginResponse = res.json().await.unwrap();
        assert!(login_response.success);
        assert!(login_response.data.is_some());
        assert!(!login_response.data.unwrap().token.is_empty());
    }

    #[tokio::test]
    async fn test_spcs_trusted_ingress_query_uses_ingress_session_without_embucket_token() {
        let rest_cfg = rest_default_cfg("json").with_trust_spcs_ingress(true);
        let execution_svc = make_test_execution_svc().await;
        let app_state = AppState {
            execution_svc,
            config: rest_cfg,
        };
        let host = "127.0.0.1:3000";
        let normalized_host = host.split_once(':').map_or(host, |(name, _)| name);
        let caller_token = make_spcs_caller_token(normalized_host);

        let app = Router::new()
            .route("/protected", post(spcs_trusted_session_probe))
            .with_state(app_state.clone())
            .layer(Extension(Host(String::default())))
            .layer(middleware::from_fn_with_state(
                app_state.clone(),
                require_auth,
            ));

        let res = app
            .oneshot(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri("/protected")
                    .header("Host", host)
                    .header(AUTHORIZATION, "Snowflake Token=\"snowflake.ingress.token\"")
                    .header("Sf-Context-Current-User", "SNOWFLAKE_USER")
                    .header("Sf-Context-Current-Account", "SNOWFLAKE_ACCOUNT")
                    .header("Sf-Context-Current-User-Token", caller_token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = res.status();
        assert_eq!(http::StatusCode::OK, status);
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["user"], "SNOWFLAKE_USER");
        assert_eq!(body["account"], "SNOWFLAKE_ACCOUNT");
        assert!(
            body["session_id"]
                .as_str()
                .is_some_and(|session_id| session_id.starts_with("spcs-"))
        );
    }

    async fn spcs_trusted_session_probe(
        State(_state): State<AppState>,
        TokenizedSession(session_id, metadata): TokenizedSession,
    ) -> Json<serde_json::Value> {
        Json(json!({
            "session_id": session_id,
            "user": metadata.attr(SessionMetadataAttr::UserName),
            "account": metadata.attr(SessionMetadataAttr::AccountName),
        }))
    }

    fn make_spcs_caller_token(audience: &str) -> String {
        let issuer = std::env::var("SNOWFLAKE_ISSUER_HOST")
            .ok()
            .or_else(|| std::env::var("SNOWFLAKE_HOST").ok())
            .unwrap_or_else(|| "snowflake-test".to_string());
        let claims = json!({
            "type": "SCT",
            "aud": audience,
            "iss": issuer,
            "callContext": "CALLER",
            "sub": "20777405349",
            "exp": time::OffsetDateTime::now_utc().unix_timestamp() + 120,
        });
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"test"),
        )
        .unwrap()
    }

    fn make_bytes_body<T: ?Sized + Serialize>(request: &T) -> Bytes {
        let json = serde_json::to_string(request).expect("Failed to serialize JSON");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(json.as_bytes())
            .expect("Failed to write to encoder");
        let compressed_data = encoder.finish().expect("Failed to finish compression");

        Bytes::from(compressed_data)
    }
}
