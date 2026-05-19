#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use crate::models::{
        JsonResponse, LoginRequestBody, LoginRequestData, LoginResponse, QueryRequestBody,
    };
    use crate::tests::create_test_server::run_test_rest_api_server;
    use crate::tests::rest_default_cfg;
    use axum::body::Bytes;
    use axum::http;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use reqwest::Method;
    use reqwest::header::AUTHORIZATION;
    use serde::Serialize;
    use serde_json::json;
    use std::collections::HashMap;
    use std::io::Write;
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
        let addr = run_test_rest_api_server(Some(rest_cfg), None).await;
        let client = reqwest::Client::new();
        let host = format!("127.0.0.1:{}", addr.port());
        let query_url = format!("http://{host}/queries/v1/query-request");
        let caller_token = make_spcs_caller_token(&host);

        let query_request = QueryRequestBody {
            sql_text: "SELECT 1;".to_string(),
            async_exec: Some(false),
            query_submission_time: Some(1_764_161_275_445),
        };

        let res = client
            .request(
                Method::POST,
                format!("{query_url}?requestId={}", Uuid::new_v4()),
            )
            .header("Content-Type", "application/json")
            .header(AUTHORIZATION, "Snowflake Token=\"snowflake.ingress.token\"")
            .header("Sf-Context-Current-User", "SNOWFLAKE_USER")
            .header("Sf-Context-Current-Account", "SNOWFLAKE_ACCOUNT")
            .header("Sf-Context-Current-User-Token", caller_token)
            .body(serde_json::to_string(&query_request).unwrap())
            .send()
            .await
            .unwrap();

        let status = res.status();
        let body = res.text().await.unwrap();
        assert_eq!(http::StatusCode::OK, status, "{body}");
        let query_response: JsonResponse = serde_json::from_str(&body).unwrap();
        assert!(query_response.success);
        assert!(query_response.data.is_some());
    }

    fn make_spcs_caller_token(audience: &str) -> String {
        let claims = json!({
            "type": "SCT",
            "aud": audience,
            "iss": "snowflake-test",
            "callContext": "CALLER",
            "sub": "SNOWFLAKE_USER",
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
