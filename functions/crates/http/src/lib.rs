//! HTTP runtime: serves a handler that speaks the [`functions_core`] envelope.
//!
//! Used behind Azure Functions custom handlers (request forwarding) and Cloud Run. Every
//! `POST`, whatever the path, is an invocation: the body is the workbench tool input and
//! the response body is the [`Response`]. Errors use the same `errorType`/`errorMessage`
//! shape as Lambda function errors, with a 4xx/5xx status that callers report as failures.

use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response as HttpResponse};
use axum::routing::post;
use functions_core::{Error, Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing_subscriber::EnvFilter;

/// Port variables set by the Azure Functions host and by Cloud Run, in that order.
const PORT_VARS: [&str; 2] = ["FUNCTIONS_CUSTOMHANDLER_PORT", "PORT"];
const DEFAULT_PORT: u16 = 8080;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    error_type: &'static str,
    error_message: String,
}

/// Serves `handler` until the process is stopped.
pub async fn run<P, R, F, Fut>(handler: F) -> std::io::Result<()>
where
    P: DeserializeOwned + Send + 'static,
    R: Serialize + Send + 'static,
    F: Fn(Request<P>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Response<R>, Error>> + Send + 'static,
{
    serve(router(handler)).await
}

/// Serves `handler` like [`run`], and runs `timer` whenever the Azure Functions host invokes
/// the timer-triggered function `timer_function` of the same app.
///
/// The host sends non-HTTP triggers to `/<function name>` in its invocation format, which
/// only reaches the handler from the host itself: HTTP requests are forwarded to the paths of
/// the HTTP triggers (`/api/...`).
pub async fn run_with_timer<P, R, F, Fut, T, TFut>(
    handler: F,
    timer_function: &str,
    timer: T,
) -> std::io::Result<()>
where
    P: DeserializeOwned + Send + 'static,
    R: Serialize + Send + 'static,
    F: Fn(Request<P>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Response<R>, Error>> + Send + 'static,
    T: Fn() -> TFut + Send + Sync + 'static,
    TFut: Future<Output = Result<(), Error>> + Send + 'static,
{
    let timer = Arc::new(timer);
    let app = router(handler).route(
        &format!("/{timer_function}"),
        post(move || {
            let timer = Arc::clone(&timer);
            async move { timer_response(timer().await) }
        }),
    );
    serve(app).await
}

fn router<P, R, F, Fut>(handler: F) -> Router
where
    P: DeserializeOwned + Send + 'static,
    R: Serialize + Send + 'static,
    F: Fn(Request<P>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Response<R>, Error>> + Send + 'static,
{
    let handler = Arc::new(handler);
    Router::new().fallback(move |method: Method, body: Bytes| {
        let handler = Arc::clone(&handler);
        async move { dispatch(handler.as_ref(), method, &body).await }
    })
}

async fn serve(app: Router) -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port()));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app).await
}

/// Custom handler invocation response: the host fails the invocation on a non-2xx status.
fn timer_response(result: Result<(), Error>) -> HttpResponse {
    let (status, logs) = match result {
        Ok(()) => (StatusCode::OK, vec![]),
        Err(err) => {
            tracing::error!(error_type = err.kind(), error = %err, "timer failed");
            (StatusCode::INTERNAL_SERVER_ERROR, vec![err.to_string()])
        }
    };
    let body = serde_json::json!({ "Outputs": {}, "Logs": logs, "ReturnValue": null });
    (status, axum::Json(body)).into_response()
}

/// HTTPS client for cloud APIs, trusting the built-in Mozilla root certificates only.
///
/// The roots are compiled in, so TLS doesn't depend on the CA certificates of the host
/// image, which can be minimal (e.g. Cloud Run's OS-only base image).
pub fn https_client() -> Result<reqwest::Client, Error> {
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|der| reqwest::Certificate::from_der(der))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| Error::provider(format!("loading root certificates: {err}")))?;
    reqwest::Client::builder()
        .tls_certs_only(roots)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| Error::provider(format!("building HTTPS client: {err}")))
}

fn port() -> u16 {
    PORT_VARS
        .iter()
        .find_map(|var| std::env::var(var).ok()?.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

async fn dispatch<P, R, F, Fut>(handler: &F, method: Method, body: &[u8]) -> HttpResponse
where
    P: DeserializeOwned,
    R: Serialize,
    F: Fn(Request<P>) -> Fut,
    Fut: Future<Output = Result<Response<R>, Error>>,
{
    if method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let result = match serde_json::from_slice::<Request<P>>(body) {
        Ok(req) => handler(req).await,
        Err(err) => Err(Error::invalid_request(err)),
    };

    match result.and_then(|resp| serde_json::to_value(resp).map_err(Error::provider)) {
        Ok(body) => (StatusCode::OK, axum::Json(body)).into_response(),
        Err(err) => {
            tracing::error!(error_type = err.kind(), error = %err, "invocation failed");
            error_response(err)
        }
    }
}

fn error_response(err: Error) -> HttpResponse {
    let status = match err {
        Error::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        Error::Provider(_) => StatusCode::BAD_GATEWAY,
    };
    let body = ErrorBody {
        error_type: err.kind(),
        error_message: err.to_string(),
    };
    (status, axum::Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use functions_core::Guard;
    use serde_json::{Map, Value, json};

    use super::*;

    async fn json_body(resp: HttpResponse) -> (StatusCode, Value) {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, body)
    }

    async fn echo(req: Request<Map<String, Value>>) -> Result<Response<Map<String, Value>>, Error> {
        match req.params.get("fail").and_then(Value::as_str) {
            Some(msg) => Err(Error::provider(msg)),
            None => Ok(Response::planned(vec![Guard::pass("ok", "ok")], req.params)),
        }
    }

    async fn call(method: Method, body: &str) -> (StatusCode, Value) {
        json_body(dispatch(&echo, method, body.as_bytes()).await).await
    }

    #[tokio::test]
    async fn returns_response_envelope() {
        let (status, body) = call(Method::POST, r#"{"volumeId":"vol-1"}"#).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["outcome"], "planned");
        assert_eq!(body["result"], json!({"volumeId": "vol-1"}));
    }

    #[tokio::test]
    async fn rejects_invalid_payload() {
        let (status, body) = call(Method::POST, r#"{"action":"delete"}"#).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["errorType"], "InvalidRequest");
    }

    #[tokio::test]
    async fn maps_provider_errors() {
        let (status, body) = call(Method::POST, r#"{"fail":"AccessDenied: nope"}"#).await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            body,
            json!({"errorType": "Provider", "errorMessage": "cloud provider error: AccessDenied: nope"})
        );
    }

    #[tokio::test]
    async fn answers_timer_invocations() {
        let (ok, body) = json_body(timer_response(Ok(()))).await;
        let (failed, err) = json_body(timer_response(Err(Error::provider("nope")))).await;

        assert_eq!(ok, StatusCode::OK);
        assert_eq!(
            body,
            json!({"Outputs": {}, "Logs": [], "ReturnValue": null})
        );
        assert_eq!(failed, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err["Logs"], json!(["cloud provider error: nope"]));
    }

    #[tokio::test]
    async fn rejects_other_methods() {
        let (status, _) = call(Method::GET, "").await;

        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }
}

#[cfg(test)]
mod client_tests {
    #[test]
    fn builds_https_client_with_builtin_roots() {
        assert!(super::https_client().is_ok());
        assert!(webpki_root_certs::TLS_SERVER_ROOT_CERTS.len() > 100);
    }
}
