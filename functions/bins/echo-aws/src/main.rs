//! Spike function used to verify the build, deploy and workbench invoke path end to end.
//!
//! It changes nothing: it echoes the tool input and reports which IAM principal the
//! function runs as, which also proves the execution role and SDK setup work.

use functions_aws::provider_error;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Echo {
    account: Option<String>,
    arn: Option<String>,
    params: Map<String, Value>,
}

async fn handle(
    sts: &aws_sdk_sts::Client,
    req: Request<Map<String, Value>>,
) -> Result<Response<Echo>, Error> {
    let identity = sts
        .get_caller_identity()
        .send()
        .await
        .map_err(provider_error)?;
    let guards = vec![Guard::pass("identity", "resolved caller identity")];
    let echo = Echo {
        account: identity.account().map(str::to_owned),
        arn: identity.arn().map(str::to_owned),
        params: req.params,
    };

    match req.action {
        Action::Plan => Ok(Response::planned(guards, echo)),
        Action::Execute => Ok(Response::done(guards, echo)),
    }
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let sts = aws_sdk_sts::Client::new(&functions_aws::sdk_config().await);

    functions_aws::run(move |req| {
        let sts = sts.clone();
        async move { handle(&sts, req).await }
    })
    .await
}
