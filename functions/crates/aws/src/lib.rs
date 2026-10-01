//! AWS Lambda glue: runs a handler that speaks the [`functions_core`] envelope.

use std::error::Error as StdError;
use std::future::Future;

use aws_config::{BehaviorVersion, SdkConfig};
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use functions_core::{Error, Request, Response};
use lambda_runtime::{Diagnostic, LambdaEvent, service_fn};
use serde::Serialize;
use serde::de::DeserializeOwned;

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

fn diagnostic(err: Error) -> Diagnostic {
    Diagnostic {
        error_type: err.kind().to_owned(),
        error_message: err.to_string(),
    }
}
