//! Spike function used to verify the build, deploy and workbench invoke path end to end.
//!
//! It changes nothing: it echoes the tool input and reports which managed identity the
//! function runs as, which also proves the identity and SDK setup work.

use functions_azure::{Identity, ManagedIdentityCredential};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Serialize)]
struct Echo {
    #[serde(flatten)]
    identity: Identity,
    params: Map<String, Value>,
}

async fn handle(
    credential: &ManagedIdentityCredential,
    req: Request<Map<String, Value>>,
) -> Result<Response<Echo>, Error> {
    let identity = functions_azure::caller_identity(credential).await?;
    let guards = vec![Guard::pass("identity", "resolved managed identity")];
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
    let credential = functions_azure::credential().map_err(std::io::Error::other)?;

    functions_http::run(move |req| {
        let credential = credential.clone();
        async move { handle(&credential, req).await }
    })
    .await
}
