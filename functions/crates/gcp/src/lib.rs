//! Google Cloud helpers: the project and access token of the running service, read from the
//! metadata server, and a minimal Compute Engine client.

pub mod compute;

use functions_core::Error;
use serde::Deserialize;

/// Metadata server host. `GCE_METADATA_HOST` overrides it, as in the Google client libraries.
const METADATA_HOST_VAR: &str = "GCE_METADATA_HOST";
const METADATA_HOST: &str = "metadata.google.internal";

/// Project the running service belongs to.
pub async fn project_id(client: &reqwest::Client) -> Result<String, Error> {
    metadata(client, &default_metadata_base(), "project/project-id").await
}

/// OAuth access token of the service's own service account.
pub async fn access_token(client: &reqwest::Client) -> Result<String, Error> {
    #[derive(Deserialize)]
    struct Token {
        access_token: String,
    }

    let body = metadata(
        client,
        &default_metadata_base(),
        "instance/service-accounts/default/token",
    )
    .await?;
    serde_json::from_str::<Token>(&body)
        .map(|t| t.access_token)
        .map_err(|err| Error::provider(format!("metadata server token: {err}")))
}

fn default_metadata_base() -> String {
    metadata_base(std::env::var(METADATA_HOST_VAR).ok())
}

fn metadata_base(host: Option<String>) -> String {
    let host = host.filter(|h| !h.is_empty());
    format!(
        "http://{}/computeMetadata/v1",
        host.as_deref().unwrap_or(METADATA_HOST)
    )
}

async fn metadata(client: &reqwest::Client, base: &str, path: &str) -> Result<String, Error> {
    let resp = client
        .get(format!("{base}/{path}"))
        .header("Metadata-Flavor", "Google")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|err| Error::provider(format!("metadata server: {err}")))?;
    let body = resp
        .text()
        .await
        .map_err(|err| Error::provider(format!("metadata server: {err}")))?;

    Ok(body.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_base_defaults_to_metadata_server() {
        assert_eq!(
            metadata_base(None),
            "http://metadata.google.internal/computeMetadata/v1"
        );
        assert_eq!(
            metadata_base(Some(String::new())),
            "http://metadata.google.internal/computeMetadata/v1"
        );
    }

    #[test]
    fn metadata_base_uses_override() {
        assert_eq!(
            metadata_base(Some("127.0.0.1:4601".into())),
            "http://127.0.0.1:4601/computeMetadata/v1"
        );
    }
}
