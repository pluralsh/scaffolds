//! Minimal Compute Engine REST client for the calls the functions make.

use std::collections::HashMap;

use functions_core::Error;
use serde::Deserialize;
use serde_json::json;

/// Compute Engine API endpoint. `GCP_COMPUTE_ENDPOINT` overrides it, e.g. for local tests.
const ENDPOINT_VAR: &str = "GCP_COMPUTE_ENDPOINT";
const ENDPOINT: &str = "https://compute.googleapis.com/compute/v1";

/// A zonal persistent disk.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub size_gb: Option<String>,
    /// Instances the disk is attached to.
    #[serde(default)]
    pub users: Vec<String>,
    /// The PD CSI driver stores the Kubernetes PV/PVC as JSON here.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// When the disk was last detached from an instance.
    #[serde(default)]
    pub last_detach_timestamp: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub creation_timestamp: Option<String>,
    /// Numeric ID of the disk the snapshot was taken of, as recorded by Compute Engine.
    #[serde(default)]
    pub source_disk_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotList {
    #[serde(default)]
    items: Vec<Snapshot>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct Operation {
    name: String,
}

pub struct Compute {
    client: reqwest::Client,
    endpoint: String,
    project: String,
    token: String,
}

impl Compute {
    /// Connects as the running service, in its own project.
    pub async fn connect(client: reqwest::Client) -> Result<Self, Error> {
        let project = crate::project_id(&client).await?;
        let token = crate::access_token(&client).await?;
        let endpoint = std::env::var(ENDPOINT_VAR)
            .ok()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| ENDPOINT.to_owned());
        Ok(Self {
            client,
            endpoint,
            project,
            token,
        })
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    /// The disk, or `None` if it doesn't exist.
    pub async fn disk(&self, zone: &str, name: &str) -> Result<Option<Disk>, Error> {
        let url = format!("{}/zones/{zone}/disks/{name}", self.project_url());
        let resp = self.send(self.client.get(url)).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::json(resp).await.map(Some)
    }

    /// Snapshots in the project carrying `label=value`, following pagination.
    pub async fn snapshots(&self, label: &str, value: &str) -> Result<Vec<Snapshot>, Error> {
        let url = format!("{}/global/snapshots", self.project_url());
        let filter = format!("labels.{label}=\"{value}\"");
        let mut snapshots = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("filter", filter.clone())];
            if let Some(token) = &page_token {
                query.push(("pageToken", token.clone()));
            }
            let resp = self.send(self.client.get(&url).query(&query)).await?;
            let page = Self::json::<SnapshotList>(resp).await?;
            snapshots.extend(page.items);
            match page.next_page_token.filter(|t| !t.is_empty()) {
                Some(token) => page_token = Some(token),
                None => return Ok(snapshots),
            }
        }
    }

    /// Starts a snapshot of the disk and returns the operation name.
    pub async fn create_snapshot(
        &self,
        zone: &str,
        disk: &str,
        name: &str,
        description: &str,
        labels: &HashMap<String, String>,
    ) -> Result<String, Error> {
        let url = format!(
            "{}/zones/{zone}/disks/{disk}/createSnapshot",
            self.project_url()
        );
        let body = json!({ "name": name, "description": description, "labels": labels });
        let resp = self.send(self.client.post(url).json(&body)).await?;
        Self::json::<Operation>(resp).await.map(|op| op.name)
    }

    /// Starts deleting the disk and returns the operation name.
    pub async fn delete_disk(&self, zone: &str, name: &str) -> Result<String, Error> {
        let url = format!("{}/zones/{zone}/disks/{name}", self.project_url());
        let resp = self.send(self.client.delete(url)).await?;
        Self::json::<Operation>(resp).await.map(|op| op.name)
    }

    fn project_url(&self) -> String {
        format!("{}/projects/{}", self.endpoint, self.project)
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
        req.bearer_auth(&self.token)
            .send()
            .await
            .map_err(|err| Error::provider(format!("compute API: {err}")))
    }

    /// Parses a successful response, or turns an API error into `<status>: <message>`.
    async fn json<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T, Error> {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|err| Error::provider(format!("compute API: {err}")))?;
        if !status.is_success() {
            return Err(Error::provider(api_error(status.as_u16(), &body)));
        }
        serde_json::from_str(&body)
            .map_err(|err| Error::provider(format!("compute API response: {err}")))
    }
}

fn api_error(status: u16, body: &str) -> String {
    #[derive(Deserialize)]
    struct Body {
        error: Detail,
    }
    #[derive(Deserialize)]
    struct Detail {
        #[serde(default)]
        status: Option<String>,
        message: String,
    }

    match serde_json::from_str::<Body>(body) {
        Ok(b) => format!(
            "{}: {}",
            b.error.status.unwrap_or_else(|| status.to_string()),
            b.error.message
        ),
        Err(_) => format!("compute API returned HTTP {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_disk() {
        let disk: Disk = serde_json::from_str(
            r#"{
                "id": "123", "name": "pvc-1", "status": "READY", "sizeGb": "10",
                "users": ["https://www.googleapis.com/compute/v1/projects/p/zones/z/instances/i"],
                "description": "{\"kubernetes.io/created-for/pvc/name\":\"data\"}",
                "labels": {"goog-gke-volume": ""}
            }"#,
        )
        .unwrap();

        assert_eq!(disk.id, "123");
        assert_eq!(disk.users.len(), 1);
        assert_eq!(disk.size_gb.as_deref(), Some("10"));
    }

    #[test]
    fn parses_minimal_disk_and_snapshot_list() {
        let disk: Disk =
            serde_json::from_str(r#"{"id": "1", "name": "d", "status": "READY"}"#).unwrap();
        let list: SnapshotList = serde_json::from_str("{}").unwrap();
        let page: SnapshotList = serde_json::from_str(
            r#"{"items": [{"name": "s", "status": "READY", "sourceDiskId": "4242"}], "nextPageToken": "t"}"#,
        )
        .unwrap();

        assert!(disk.users.is_empty() && disk.description.is_none());
        assert!(disk.last_detach_timestamp.is_none());
        assert!(list.items.is_empty() && list.next_page_token.is_none());
        assert_eq!(page.items[0].source_disk_id.as_deref(), Some("4242"));
        assert_eq!(page.next_page_token.as_deref(), Some("t"));
    }

    #[test]
    fn formats_api_errors() {
        assert_eq!(
            api_error(
                400,
                r#"{"error": {"code": 400, "status": "FAILED_PRECONDITION", "message": "disk in use"}}"#
            ),
            "FAILED_PRECONDITION: disk in use"
        );
        assert_eq!(api_error(502, "<html>"), "compute API returned HTTP 502");
    }
}
