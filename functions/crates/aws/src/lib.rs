//! AWS Lambda glue: runs a handler that speaks the [`functions_core`] envelope.

use std::error::Error as StdError;
use std::future::Future;

use aws_config::{BehaviorVersion, SdkConfig};
use aws_smithy_types::error::display::DisplayErrorContext;
use functions_core::{Error, Request, Response};
use lambda_runtime::{LambdaEvent, service_fn};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Loads SDK configuration from the Lambda environment (execution role, region).
pub async fn sdk_config() -> SdkConfig {
    aws_config::load_defaults(BehaviorVersion::latest()).await
}

/// Converts an AWS SDK error into [`Error::Provider`], keeping the full cause chain.
pub fn provider_error<E: StdError>(err: E) -> Error {
    Error::provider(DisplayErrorContext(err))
}

/// Starts the Lambda runtime loop with `handler`.
///
/// Guard refusals are regular responses. Only an [`Error`] is surfaced as a Lambda
/// function error, which the workbench shows as a failed tool call.
pub async fn run<P, R, F, Fut>(handler: F) -> Result<(), lambda_runtime::Error>
where
    P: DeserializeOwned + Send + 'static,
    R: Serialize + Send + 'static,
    F: Fn(Request<P>) -> Fut,
    Fut: Future<Output = Result<Response<R>, Error>> + Send,
{
    lambda_runtime::tracing::init_default_subscriber();

    lambda_runtime::run(service_fn(move |event: LambdaEvent<Request<P>>| {
        let action = event.payload.action;
        let fut = handler(event.payload);
        async move {
            fut.await.map_err(|err| {
                tracing::error!(?action, error = %err, "invocation failed");
                lambda_runtime::Error::from(err)
            })
        }
    }))
    .await
}
