//! In-process mock of Azure Resource Manager for handler tests.
//!
//! Tests script responses by method and path with [`MockArm::on`], run a handler against
//! [`MockArm::connector`] and inspect the requests it made. Anything not scripted gets an ARM
//! `404 ResourceNotFound`, so a test only describes the resources that exist.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use azure_core::credentials::{AccessToken, TokenCredential, TokenRequestOptions};
use azure_core::time::OffsetDateTime;
use serde_json::{Value, json};

use crate::arm::Connector;

pub use axum::http::Method;

/// A request the handler sent.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: Method,
    /// Path without the query string.
    pub path: String,
    pub query: String,
    pub if_match: Option<String>,
    pub if_none_match: Option<String>,
    pub body: Value,
}

#[derive(Debug, Clone)]
struct Reply {
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: Value,
}

#[derive(Default)]
struct State {
    /// Replies per `METHOD path`, served in order; the last one repeats.
    replies: HashMap<String, Vec<Reply>>,
    /// Replies to any path of a method that has no reply of its own.
    fallbacks: HashMap<String, Reply>,
    requests: Vec<Recorded>,
}

#[derive(Clone)]
pub struct MockArm {
    url: String,
    state: Arc<Mutex<State>>,
}

impl MockArm {
    /// Starts the mock on a free local port.
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        let app = Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                let state = Arc::clone(&shared);
                async move { serve(&state, method, &uri, &headers, &body) }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("binding the mock ARM endpoint");
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { url, state }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// A connector to the mock with a fixed test token.
    pub fn connector(&self) -> Connector {
        Connector::new(
            reqwest::Client::new(),
            Arc::new(StaticCredential),
            self.url.clone(),
        )
    }

    /// Answers `method path` with `status` and `body`. Calling it again for the same request
    /// queues another reply.
    pub fn on(&self, method: Method, path: &str, status: u16, body: Value) -> &Self {
        self.on_with_headers(method, path, status, &[], body)
    }

    pub fn on_with_headers(
        &self,
        method: Method,
        path: &str,
        status: u16,
        headers: &[(&str, &str)],
        body: Value,
    ) -> &Self {
        let reply = Reply {
            status: StatusCode::from_u16(status).expect("valid status"),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            body,
        };
        self.state
            .lock()
            .unwrap()
            .replies
            .entry(key(&method, path))
            .or_default()
            .push(reply);
        self
    }

    /// Answers every `method` request to a path without replies of its own, e.g. writes to
    /// names the handler generates.
    pub fn on_any(&self, method: Method, status: u16, body: Value) -> &Self {
        let reply = Reply {
            status: StatusCode::from_u16(status).expect("valid status"),
            headers: vec![],
            body,
        };
        self.state
            .lock()
            .unwrap()
            .fallbacks
            .insert(method.to_string(), reply);
        self
    }

    /// Every request the handler sent, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Every non-GET request, in order. This includes read-only POST actions such as health
    /// checks.
    pub fn writes(&self) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.method != Method::GET)
            .collect()
    }
}

fn key(method: &Method, path: &str) -> String {
    format!("{method} {}", path.to_lowercase())
}

fn serve(
    state: &Mutex<State>,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let mut state = state.lock().unwrap();
    state.requests.push(Recorded {
        method: method.clone(),
        path: uri.path().to_owned(),
        query: uri.query().unwrap_or_default().to_owned(),
        if_match: header("if-match"),
        if_none_match: header("if-none-match"),
        body: serde_json::from_slice(body).unwrap_or(Value::Null),
    });
    let reply = state
        .replies
        .get_mut(&key(&method, uri.path()))
        .and_then(|replies| {
            if replies.len() > 1 {
                Some(replies.remove(0))
            } else {
                replies.first().cloned()
            }
        })
        .or_else(|| state.fallbacks.get(method.as_str()).cloned());
    match reply {
        Some(reply) => {
            let mut resp = (reply.status, axum::Json(reply.body)).into_response();
            for (k, v) in reply.headers {
                resp.headers_mut().insert(
                    axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                    v.parse().unwrap(),
                );
            }
            resp
        }
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"error": {
                "code": "ResourceNotFound",
                "message": format!("{method} {} is not scripted", uri.path()),
            }})),
        )
            .into_response(),
    }
}

/// Credential returning a fixed token.
#[derive(Debug)]
pub struct StaticCredential;

#[async_trait::async_trait]
impl TokenCredential for StaticCredential {
    async fn get_token(
        &self,
        _scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        Ok(AccessToken::new(
            "test-token",
            OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600),
        ))
    }
}
