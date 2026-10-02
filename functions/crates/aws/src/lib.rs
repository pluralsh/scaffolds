//! AWS Lambda glue: runs a handler that speaks the [`functions_core`] envelope.

use std::error::Error as StdError;
use std::future::Future;
use std::sync::Arc;

use aws_config::{BehaviorVersion, SdkConfig};
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use functions_core::{Error, Request, Response};
use lambda_runtime::{Diagnostic, LambdaEvent, service_fn};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Loads SDK configuration from the Lambda environment (execution role, region).
pub async fn sdk_config() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Converts an AWS SDK error into [`Error::Provider`].
///
/// Service errors are reduced to `<code>: <message>` (e.g. `AccessDenied: ...`) instead of the
/// raw HTTP response. Errors without a code, such as connection failures, keep their full
/// cause chain.
pub fn provider_error<E: StdError + ProvideErrorMetadata>(err: E) -> Error {
    match (err.code(), err.message()) {
        (Some(code), Some(message)) => Error::provider(format!("{code}: {message}")),
        (Some(code), None) => Error::provider(code),
        _ => Error::provider(DisplayErrorContext(err)),
    }
}

/// Starts the Lambda runtime loop with `handler`.
///
/// Guard refusals are normal responses. Only an [`Error`] becomes a Lambda function error,
/// typed with [`Error::kind`]. The workbench shows it as a failed tool call.
pub async fn run<P, R, F, Fut>(handler: F) -> Result<(), lambda_runtime::Error>
where
    P: DeserializeOwned + Send + 'static,
    R: Serialize + Send + 'static,
    F: Fn(Request<P>) -> Fut,
    Fut: Future<Output = Result<Response<R>, Error>> + Send,
{
    lambda_runtime::tracing::init_default_subscriber();

    lambda_runtime::run(service_fn(move |event: LambdaEvent<Request<P>>| {
        let fut = handler(event.payload);
        async move { fut.await.map_err(diagnostic) }
    }))
    .await
}

/// Like [`run`], and also runs `expire` when the event is `{"pluralExpire": true}`.
///
/// Used for functions that need a periodic cleanup, invoked by EventBridge with that payload
/// so the workbench tool schema stays unchanged.
pub async fn run_with_expire<P, R, F, Fut, E, EFut>(
    handler: F,
    expire: E,
) -> Result<(), lambda_runtime::Error>
where
    P: DeserializeOwned + Send + 'static,
    R: Serialize + Send + 'static,
    F: Fn(Request<P>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Response<R>, Error>> + Send + 'static,
    E: Fn() -> EFut + Send + Sync + 'static,
    EFut: Future<Output = Result<(), Error>> + Send + 'static,
{
    lambda_runtime::tracing::init_default_subscriber();

    let handler = Arc::new(handler);
    let expire = Arc::new(expire);

    lambda_runtime::run(service_fn(move |event: LambdaEvent<Value>| {
        let handler = Arc::clone(&handler);
        let expire = Arc::clone(&expire);
        let payload = event.payload;
        async move {
            if payload.get("pluralExpire").and_then(Value::as_bool) == Some(true) {
                expire().await.map_err(diagnostic)?;
                return Ok(Value::Object(serde_json::Map::new()));
            }
            let req: Request<P> = serde_json::from_value(payload).map_err(|err| {
                diagnostic(Error::invalid_request(format!("invalid request: {err}")))
            })?;
            let response = handler(req).await.map_err(diagnostic)?;
            serde_json::to_value(response).map_err(|err| diagnostic(Error::provider(err)))
        }
    }))
    .await
}

fn diagnostic(err: Error) -> Diagnostic {
    Diagnostic {
        error_type: err.kind().to_owned(),
        error_message: err.to_string(),
    }
}
