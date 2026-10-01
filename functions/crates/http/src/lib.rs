//! HTTP runtime: serves a handler that speaks the [`functions_core`] envelope.
//!
//! Used behind Azure Functions custom handlers (request forwarding). Every `POST`, on any
//! path, is an invocation. The body is the workbench tool input and the response body is the
//! [`Response`]. Errors use the `errorType`/`errorMessage` shape of Lambda function errors,
//! with a 4xx/5xx status that callers report as a failure.
//!
//! Started with an argument, the binary instead runs once, for local runs: the argument is the
//! input JSON (`-` reads it from stdin), or the name of the app's timer function to run that.
//! The answer goes to stdout, logs to stderr. The Functions host starts it without arguments.

use std::future::Future;
use std::io::Read;
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

/// Port variables set by the Azure Functions host and, as a fallback for local runs, `PORT`.
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
    match Once::from_args(None)? {
        Some(Once::Invoke(body)) => {
            init_logging(true);
            exit_with(invoke(&handler, &body).await)
        }
        Some(Once::Timer) => unreachable!("no timer function"),
        None => serve(router(handler)).await,
    }
}

/// Serves `handler` like [`run`], and runs `timer` whenever the Azure Functions host invokes
/// the app's timer-triggered function `timer_function`.
///
/// The host sends non-HTTP triggers to `/<function name>` in its invocation format. Only the
/// host can call that path, since outside HTTP requests are forwarded to the HTTP trigger
/// paths (`/api/...`).
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
    match Once::from_args(Some(timer_function))? {
        Some(Once::Invoke(body)) => {
            init_logging(true);
            exit_with(invoke(&handler, &body).await)
        }
        Some(Once::Timer) => {
            init_logging(true);
            exit_with(timer().await.map(|()| serde_json::Value::Null))
        }
        None => {}
    }

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
    init_logging(false);
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port()));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app).await
}

/// JSON logs on stdout for the Functions host, or readable ones on stderr for a single run.
fn init_logging(once: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    if once {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    }
}

/// A single run requested on the command line.
#[derive(Debug, PartialEq)]
enum Once {
    /// Invoke the handler with this input.
    Invoke(Vec<u8>),
    /// Run the timer function.
    Timer,
}

impl Once {
    fn from_args(timer_function: Option<&str>) -> std::io::Result<Option<Self>> {
        Self::parse(std::env::args().nth(1), timer_function, std::io::stdin())
    }

    fn parse(
        arg: Option<String>,
        timer_function: Option<&str>,
        mut stdin: impl Read,
    ) -> std::io::Result<Option<Self>> {
        Ok(match arg {
            None => None,
            Some(arg) if Some(arg.as_str()) == timer_function => Some(Self::Timer),
            Some(arg) if arg == "-" => {
                let mut body = Vec::new();
                stdin.read_to_end(&mut body)?;
                Some(Self::Invoke(body))
            }
            Some(arg) => Some(Self::Invoke(arg.into_bytes())),
        })
    }
}

/// Prints the answer of a single run and exits: 0 when it succeeded, 2 for an invalid request,
/// 1 for any other error.
fn exit_with(result: Result<serde_json::Value, Error>) -> ! {
    let (code, body) = match result {
        Ok(body) => (0, body),
        Err(err) => {
            let code = match err {
                Error::InvalidRequest(_) => 2,
                Error::Provider(_) => 1,
            };
            let body = serde_json::to_value(error_body(&err)).unwrap_or_default();
            (code, body)
        }
    };
    if !body.is_null() {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    }
    std::process::exit(code)
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
/// The roots are compiled in, so TLS works on minimal host images without CA certificates.
pub fn https_client() -> Result<reqwest::Client, Error> {
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|der| reqwest::Certificate::from_der(der))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| Error::provider(format!("loading root certificates: {err}")))?;
    reqwest::Client::builder()
        .tls_certs_only(roots)
        // Cloud APIs answer within seconds and run long operations asynchronously. An
        // invocation makes a few requests and must fit in the 30 seconds callers wait.
        .timeout(std::time::Duration::from_secs(8))
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

    match invoke(handler, body).await {
        Ok(body) => (StatusCode::OK, axum::Json(body)).into_response(),
        Err(err) => error_response(err),
    }
}

/// Parses `body`, runs `handler` on it and serializes its response.
async fn invoke<P, R, F, Fut>(handler: &F, body: &[u8]) -> Result<serde_json::Value, Error>
where
    P: DeserializeOwned,
    R: Serialize,
    F: Fn(Request<P>) -> Fut,
    Fut: Future<Output = Result<Response<R>, Error>>,
{
    let result = match serde_json::from_slice::<Request<P>>(body) {
        Ok(req) => handler(req).await,
        Err(err) => Err(Error::invalid_request(err)),
    };
    let result = result.and_then(|resp| serde_json::to_value(resp).map_err(Error::provider));
    if let Err(err) = &result {
        tracing::error!(error_type = err.kind(), error = %err, "invocation failed");
    }
    result
}

fn error_response(err: Error) -> HttpResponse {
    let status = match err {
        Error::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        Error::Provider(_) => StatusCode::BAD_GATEWAY,
    };
    (status, axum::Json(error_body(&err))).into_response()
}

fn error_body(err: &Error) -> ErrorBody {
    ErrorBody {
        error_type: err.kind(),
        error_message: err.to_string(),
    }
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

    #[test]
    fn parses_single_run_arguments() {
        let parse = |arg: Option<&str>, stdin: &str| {
            Once::parse(arg.map(str::to_owned), Some("sweep"), stdin.as_bytes()).unwrap()
        };

        assert_eq!(parse(None, ""), None);
        assert_eq!(parse(Some("sweep"), ""), Some(Once::Timer));
        assert_eq!(
            parse(Some(r#"{"a":1}"#), ""),
            Some(Once::Invoke(br#"{"a":1}"#.to_vec()))
        );
        assert_eq!(
            parse(Some("-"), r#"{"b":2}"#),
            Some(Once::Invoke(br#"{"b":2}"#.to_vec()))
        );
        // Without a timer function, its name is just an (invalid) input.
        assert_eq!(
            Once::parse(Some("sweep".into()), None, std::io::empty()).unwrap(),
            Some(Once::Invoke(b"sweep".to_vec()))
        );
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
