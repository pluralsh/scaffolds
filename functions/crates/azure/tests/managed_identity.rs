//! The real managed identity credential against a local App Service identity endpoint, the
//! way Azure Functions provides it.

#![cfg(feature = "mock")]

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use functions_azure::ARM_SCOPE;
use functions_azure::arm::Connector;
use serde_json::{Value, json};

/// What the credential sent: the query and the identity header.
type Seen = Arc<Mutex<Vec<(Vec<(String, String)>, Option<String>)>>>;

async fn token(
    State(seen): State<Seen>,
    Query(query): Query<Vec<(String, String)>>,
    headers: HeaderMap,
) -> axum::Json<Value> {
    let header = headers
        .get("x-identity-header")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    seen.lock().unwrap().push((query, header));
    let expires_on = (std::time::SystemTime::now() + std::time::Duration::from_secs(3600))
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    axum::Json(json!({
        "access_token": "managed-identity-token",
        "expires_on": expires_on.to_string(),
        "resource": "https://management.azure.com",
        "token_type": "Bearer",
    }))
}

#[tokio::test]
async fn gets_arm_tokens_from_the_function_apps_identity_endpoint() {
    let seen = Seen::default();
    let app = Router::new()
        .route("/msi/token", get(token))
        .with_state(Arc::clone(&seen));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/msi/token", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });

    // SAFETY: this test binary runs this single test, so no other thread reads the
    // environment meanwhile.
    unsafe {
        std::env::set_var("IDENTITY_ENDPOINT", &endpoint);
        std::env::set_var("IDENTITY_HEADER", "identity-secret");
    }
    let credential = functions_azure::credential().unwrap();
    let connector = Connector::new(reqwest::Client::new(), credential.clone(), "http://unused");

    connector.connect().await.unwrap();
    let token =
        azure_core::credentials::TokenCredential::get_token(&*credential, &[ARM_SCOPE], None)
            .await
            .unwrap();

    assert_eq!(token.token.secret(), "managed-identity-token");
    // The second token comes from the credential's cache.
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let (query, header) = &seen[0];
    assert_eq!(header.as_deref(), Some("identity-secret"));
    assert!(
        query.contains(&("api-version".into(), "2019-08-01".into())),
        "{query:?}"
    );
    assert!(
        query.contains(&("resource".into(), "https://management.azure.com".into())),
        "{query:?}"
    );
    // A system-assigned identity: no client ID is asked for.
    assert!(
        !query.iter().any(|(k, _)| k.contains("client_id")),
        "{query:?}"
    );
}
