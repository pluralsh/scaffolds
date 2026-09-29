//! Spike function used to verify the build, deploy and workbench invoke path end to end.
//!
//! It changes nothing: it echoes the tool input and reports which service account the
//! Cloud Run service runs as, which also proves the service identity setup works.

use functions_core::{Action, Error, Guard, Request, Response};
use functions_gcp::Identity;
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Serialize)]
struct Echo {
    #[serde(flatten)]
    identity: Identity,
    params: Map<String, Value>,
}

async fn handle(
    client: &reqwest::Client,
    req: Request<Map<String, Value>>,
) -> Result<Response<Echo>, Error> {
    let identity = functions_gcp::caller_identity(client).await?;
    let guards = vec![Guard::pass("identity", "resolved service account")];
    let echo = Echo {
        identity,
        params: req.params,
    };

    match req.action {
        Action::Plan => Ok(Response::planned(guards, echo)),
        Action::Execute => Ok(Response::done(guards, echo)),
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = reqwest::Client::new();

    functions_http::run(move |req| {
        let client = client.clone();
        async move { handle(&client, req).await }
    })
    .await
}
