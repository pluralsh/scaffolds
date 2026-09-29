//! Google Cloud helpers: the identity functions run as, read from the metadata server.

use functions_core::Error;
use serde::Serialize;

/// Metadata server host. `GCE_METADATA_HOST` overrides it, as in the Google client libraries.
const METADATA_HOST_VAR: &str = "GCE_METADATA_HOST";
const METADATA_HOST: &str = "metadata.google.internal";

/// Identity a Cloud Run service runs as.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub service_account: String,
    pub project_id: String,
}

/// Resolves the service account and project of the running service.
///
/// The metadata server needs no IAM permissions, so this also verifies the service
/// identity setup without granting any.
pub async fn caller_identity(client: &reqwest::Client) -> Result<Identity, Error> {
    let base = metadata_base(std::env::var(METADATA_HOST_VAR).ok());

    Ok(Identity {
        service_account: metadata(client, &base, "instance/service-accounts/default/email").await?,
        project_id: metadata(client, &base, "project/project-id").await?,
    })
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
