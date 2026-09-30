//! Minimal Azure Resource Manager client for the calls the functions make.

use std::collections::HashMap;

use azure_core::credentials::TokenCredential;
use functions_core::Error;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::{ARM_SCOPE, provider_error};

/// ARM endpoint. `AZURE_ARM_ENDPOINT` overrides it, e.g. for local tests.
const ENDPOINT_VAR: &str = "AZURE_ARM_ENDPOINT";
const ENDPOINT: &str = "https://management.azure.com";

/// API version of Microsoft.Compute disks and snapshots.
pub const COMPUTE_API_VERSION: &str = "2024-03-02";

/// A managed disk, identified by `/subscriptions/<s>/resourceGroups/<rg>/providers/Microsoft.Compute/disks/<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskId {
    pub subscription: String,
    pub resource_group: String,
    pub name: String,
}

impl DiskId {
    /// Parses a disk resource ID. Only this exact shape is accepted, so the ID can't point
    /// the client at a different resource or API.
    pub fn parse(id: &str) -> Option<Self> {
        let parts: Vec<&str> = id.strip_prefix('/')?.split('/').collect();
        let [
            sub_key,
            subscription,
            rg_key,
            resource_group,
            providers,
            namespace,
            kind,
            name,
        ] = parts.as_slice()
        else {
            return None;
        };
        let ok = sub_key.eq_ignore_ascii_case("subscriptions")
            && is_guid(subscription)
            && rg_key.eq_ignore_ascii_case("resourceGroups")
            && is_resource_group(resource_group)
            && providers.eq_ignore_ascii_case("providers")
            && namespace.eq_ignore_ascii_case("Microsoft.Compute")
            && kind.eq_ignore_ascii_case("disks")
            && is_disk_name(name);
        ok.then(|| Self {
            subscription: subscription.to_lowercase(),
            resource_group: (*resource_group).to_owned(),
            name: (*name).to_owned(),
        })
    }

    /// The canonical resource ID, rebuilt from the parsed parts.
    pub fn id(&self) -> String {
        format!(
            "{}/providers/Microsoft.Compute/disks/{}",
            self.resource_group_id(),
            self.name
        )
    }

    pub fn resource_group_id(&self) -> String {
        format!(
            "/subscriptions/{}/resourceGroups/{}",
            self.subscription, self.resource_group
        )
    }
}

fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// 1-90 of letters, digits, `-`, `_`, `.`, `(`, `)`, not ending in `.`.
fn is_resource_group(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 90
        && !s.ends_with('.')
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '(' | ')'))
}

/// 1-80 of letters, digits, `-`, `_`, `.`, starting with a letter or digit, ending with a
/// letter, digit or `_`.
fn is_disk_name(s: &str) -> bool {
    let first_ok = s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    let last_ok = s
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    s.len() <= 80
        && first_ok
        && last_ok
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub id: String,
    pub name: String,
    pub location: String,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    #[serde(default)]
    pub managed_by: Option<String>,
    #[serde(default)]
    pub managed_by_extended: Vec<String>,
    #[serde(default)]
    pub sku: Option<Sku>,
    #[serde(default)]
    pub properties: DiskProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Sku {
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskProperties {
    #[serde(default)]
    pub disk_state: String,
    #[serde(default)]
    pub provisioning_state: String,
    #[serde(default, rename = "diskSizeGB")]
    pub disk_size_gb: Option<i64>,
    /// Stable ID of this disk; a disk re-created with the same name gets a new one.
    #[serde(default)]
    pub unique_id: Option<String>,
    /// When the disk was last attached or detached.
    #[serde(default)]
    pub last_ownership_update_time: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub name: String,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    #[serde(default)]
    pub properties: SnapshotProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotProperties {
    #[serde(default)]
    pub provisioning_state: String,
    /// Background copy progress of incremental snapshots.
    #[serde(default)]
    pub completion_percent: Option<f64>,
    #[serde(default)]
    pub time_created: Option<String>,
    #[serde(default)]
    pub creation_data: Option<CreationData>,
}

/// The source a snapshot was taken of, as recorded by Azure.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationData {
    #[serde(default)]
    pub source_resource_id: Option<String>,
    /// `uniqueId` of the source disk.
    #[serde(default)]
    pub source_unique_id: Option<String>,
}

#[derive(Deserialize)]
struct List<T> {
    #[serde(default = "Vec::new")]
    value: Vec<T>,
    #[serde(default, rename = "nextLink")]
    next_link: Option<String>,
}

pub struct Arm {
    client: reqwest::Client,
    endpoint: String,
    token: String,
}

impl Arm {
    /// Connects to ARM with a token of `credential`.
    pub async fn connect(
        client: reqwest::Client,
        credential: &dyn TokenCredential,
    ) -> Result<Self, Error> {
        let token = credential
            .get_token(&[ARM_SCOPE], None)
            .await
            .map_err(provider_error)?;
        let endpoint = std::env::var(ENDPOINT_VAR)
            .ok()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| ENDPOINT.to_owned());
        Ok(Self {
            client,
            endpoint,
            token: token.token.secret().to_owned(),
        })
    }

    /// The disk, or `None` if it doesn't exist.
    pub async fn disk(&self, id: &DiskId) -> Result<Option<Disk>, Error> {
        let resp = self.send(self.client.get(self.url(&id.id()))).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        json(resp).await.map(Some)
    }

    /// Snapshots in the disk's resource group, following pagination.
    pub async fn snapshots(&self, id: &DiskId) -> Result<Vec<Snapshot>, Error> {
        let mut url = self.url(&format!(
            "{}/providers/Microsoft.Compute/snapshots",
            id.resource_group_id()
        ));
        let mut snapshots = Vec::new();
        loop {
            let page: List<Snapshot> = json(self.send(self.client.get(&url)).await?).await?;
            snapshots.extend(page.value);
            match page.next_link {
                // Only follow links back to the same endpoint, as they carry the token.
                Some(next) if same_origin(&next, &self.endpoint) => url = next,
                _ => return Ok(snapshots),
            }
        }
    }

    /// Starts an incremental snapshot of the disk in its resource group.
    pub async fn create_snapshot(
        &self,
        disk: &DiskId,
        location: &str,
        name: &str,
        tags: &HashMap<String, String>,
    ) -> Result<(), Error> {
        let path = format!(
            "{}/providers/Microsoft.Compute/snapshots/{name}",
            disk.resource_group_id()
        );
        let body = serde_json::json!({
            "location": location,
            "tags": tags,
            "properties": {
                "creationData": { "createOption": "Copy", "sourceResourceId": disk.id() },
                "incremental": true,
            },
        });
        let resp = self
            .send(self.client.put(self.url(&path)).json(&body))
            .await?;
        ensure_success(resp).await
    }

    /// Starts deleting the disk.
    pub async fn delete_disk(&self, id: &DiskId) -> Result<(), Error> {
        let resp = self.send(self.client.delete(self.url(&id.id()))).await?;
        ensure_success(resp).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}?api-version={COMPUTE_API_VERSION}", self.endpoint)
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
        req.bearer_auth(&self.token)
            .send()
            .await
            .map_err(|err| Error::provider(format!("ARM: {err}")))
    }
}

/// Whether `url` has the same scheme, host and port as `endpoint`.
fn same_origin(url: &str, endpoint: &str) -> bool {
    match (reqwest::Url::parse(url), reqwest::Url::parse(endpoint)) {
        (Ok(a), Ok(b)) => {
            a.scheme() == b.scheme()
                && a.host_str() == b.host_str()
                && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

async fn json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, Error> {
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|err| Error::provider(format!("ARM: {err}")))?;
    if !status.is_success() {
        return Err(Error::provider(api_error(status.as_u16(), &body)));
    }
    serde_json::from_str(&body).map_err(|err| Error::provider(format!("ARM response: {err}")))
}

async fn ensure_success(resp: reqwest::Response) -> Result<(), Error> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().await.unwrap_or_default();
    Err(Error::provider(api_error(status.as_u16(), &body)))
}

/// Turns an ARM error body into `<code>: <message>`.
fn api_error(status: u16, body: &str) -> String {
    #[derive(Deserialize)]
    struct Body {
        error: Detail,
    }
    #[derive(Deserialize)]
    struct Detail {
        code: String,
        message: String,
    }

    match serde_json::from_str::<Body>(body) {
        Ok(b) => format!("{}: {}", b.error.code, b.error.message),
        Err(_) => format!("ARM returned HTTP {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUB: &str = "00000000-0000-0000-0000-000000000000";

    #[test]
    fn parses_and_rebuilds_disk_ids() {
        let id = DiskId::parse(&format!(
            "/subscriptions/{SUB}/resourceGroups/MC_rg_aks_eastus/providers/microsoft.compute/disks/pvc-1"
        ))
        .unwrap();

        assert_eq!(id.resource_group, "MC_rg_aks_eastus");
        assert_eq!(
            id.id(),
            format!(
                "/subscriptions/{SUB}/resourceGroups/MC_rg_aks_eastus/providers/Microsoft.Compute/disks/pvc-1"
            )
        );
    }

    #[test]
    fn rejects_other_resource_ids() {
        for id in [
            "".to_owned(),
            "pvc-1".to_owned(),
            format!(
                "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/snapshots/s"
            ),
            format!(
                "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/disks/d/extra"
            ),
            format!(
                "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/disks/../x"
            ),
            format!(
                "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/disks/d?api-version=1"
            ),
            "/subscriptions/not-a-guid/resourceGroups/rg/providers/Microsoft.Compute/disks/d"
                .to_owned(),
            format!("subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/disks/d"),
        ] {
            assert_eq!(DiskId::parse(&id), None, "{id}");
        }
    }

    #[test]
    fn parses_disk_and_snapshot_list() {
        let disk: Disk = serde_json::from_str(
            r#"{
                "id": "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/pvc-1",
                "name": "pvc-1", "location": "eastus",
                "tags": {"kubernetes.io-created-for-pvc-name": "data"},
                "managedBy": "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm-1",
                "properties": {"diskState": "Attached", "provisioningState": "Succeeded", "diskSizeGB": 32}
            }"#,
        )
        .unwrap();
        let list: List<Snapshot> = serde_json::from_str(
            r#"{"value": [{"name": "s", "properties": {"provisioningState": "Succeeded", "completionPercent": 42.5, "timeCreated": "2026-09-30T10:00:00.1234567+00:00"}}]}"#,
        )
        .unwrap();

        assert_eq!(disk.properties.disk_state, "Attached");
        assert_eq!(disk.properties.disk_size_gb, Some(32));
        assert!(disk.managed_by.is_some() && disk.managed_by_extended.is_empty());
        assert_eq!(list.value[0].properties.completion_percent, Some(42.5));
        assert!(list.next_link.is_none());
    }

    #[test]
    fn follows_next_links_to_the_same_endpoint_only() {
        let arm = "https://management.azure.com";

        assert!(same_origin(
            "https://management.azure.com/subscriptions/s?$skiptoken=1",
            arm
        ));
        for next in [
            "https://management.azure.com.evil.tld/subscriptions/s",
            "http://management.azure.com/subscriptions/s",
            "https://management.azure.com:8443/subscriptions/s",
            "https://evil.tld/https://management.azure.com",
            "not a url",
        ] {
            assert!(!same_origin(next, arm), "{next}");
        }
    }

    #[test]
    fn parses_snapshot_source() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{"name": "s", "properties": {"provisioningState": "Succeeded", "creationData": {"createOption": "Copy", "sourceResourceId": "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/d", "sourceUniqueId": "u-1"}}}"#,
        )
        .unwrap();
        let disk: Disk = serde_json::from_str(
            r#"{"id": "i", "name": "d", "location": "eastus", "sku": {"name": "PremiumV2_LRS"}, "properties": {"uniqueId": "u-1", "lastOwnershipUpdateTime": "2026-09-30T10:00:00Z"}}"#,
        )
        .unwrap();

        assert_eq!(
            snapshot
                .properties
                .creation_data
                .unwrap()
                .source_unique_id
                .as_deref(),
            Some("u-1")
        );
        assert_eq!(disk.sku.unwrap().name, "PremiumV2_LRS");
        assert_eq!(disk.properties.unique_id.as_deref(), Some("u-1"));
        assert!(disk.properties.last_ownership_update_time.is_some());
    }

    #[test]
    fn formats_api_errors() {
        assert_eq!(
            api_error(
                409,
                r#"{"error": {"code": "OperationNotAllowed", "message": "Disk is attached"}}"#
            ),
            "OperationNotAllowed: Disk is attached"
        );
        assert_eq!(api_error(500, ""), "ARM returned HTTP 500");
    }
}
