//! Minimal Azure Resource Manager client for the calls the functions make.

use std::collections::HashMap;
use std::sync::Arc;

use azure_core::credentials::TokenCredential;
use functions_core::Error;
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{ARM_SCOPE, provider_error};

/// ARM endpoint. `AZURE_ARM_ENDPOINT` overrides it, e.g. for local tests.
const ENDPOINT_VAR: &str = "AZURE_ARM_ENDPOINT";
const ENDPOINT: &str = "https://management.azure.com";

/// Longest wait between two polls of a long-running operation.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// API version of Microsoft.Compute disks and snapshots.
pub const COMPUTE_API_VERSION: &str = "2024-03-02";

/// A resource type the functions accept IDs of, with its canonical spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub namespace: &'static str,
    pub kind: &'static str,
}

pub const DISK: Kind = Kind {
    namespace: "Microsoft.Compute",
    kind: "disks",
};
pub const VIRTUAL_MACHINE: Kind = Kind {
    namespace: "Microsoft.Compute",
    kind: "virtualMachines",
};
pub const MANAGED_CLUSTER: Kind = Kind {
    namespace: "Microsoft.ContainerService",
    kind: "managedClusters",
};
pub const LOAD_BALANCER: Kind = Kind {
    namespace: "Microsoft.Network",
    kind: "loadBalancers",
};
pub const PUBLIC_IP: Kind = Kind {
    namespace: "Microsoft.Network",
    kind: "publicIPAddresses",
};
pub const BASTION_HOST: Kind = Kind {
    namespace: "Microsoft.Network",
    kind: "bastionHosts",
};
pub const POSTGRES_FLEXIBLE_SERVER: Kind = Kind {
    namespace: "Microsoft.DBforPostgreSQL",
    kind: "flexibleServers",
};
pub const MYSQL_FLEXIBLE_SERVER: Kind = Kind {
    namespace: "Microsoft.DBforMySQL",
    kind: "flexibleServers",
};

/// A top-level resource in a resource group, identified by
/// `/subscriptions/<s>/resourceGroups/<rg>/providers/<namespace>/<kind>/<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceId {
    pub subscription: String,
    pub resource_group: String,
    pub kind: Kind,
    pub name: String,
}

impl ResourceId {
    /// Parses a resource ID of one of `kinds`. Only this exact shape is accepted, so the ID
    /// can't point the client at a different resource or API: the ID is rebuilt from the
    /// parsed parts, and names can't contain `/`, `?`, `#`, `%` or be `.` or `..`.
    pub fn parse(id: &str, kinds: &[Kind]) -> Option<Self> {
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
        let kind = kinds.iter().find(|k| {
            namespace.eq_ignore_ascii_case(k.namespace) && kind.eq_ignore_ascii_case(k.kind)
        })?;
        let ok = sub_key.eq_ignore_ascii_case("subscriptions")
            && is_guid(subscription)
            && rg_key.eq_ignore_ascii_case("resourceGroups")
            && is_resource_group(resource_group)
            && providers.eq_ignore_ascii_case("providers")
            && is_name(name);
        ok.then(|| Self {
            subscription: subscription.to_lowercase(),
            resource_group: (*resource_group).to_owned(),
            kind: *kind,
            name: (*name).to_owned(),
        })
    }

    /// A resource of `kind` named `name` in the same resource group.
    pub fn sibling(&self, kind: Kind, name: &str) -> Option<Self> {
        is_name(name).then(|| Self {
            kind,
            name: name.to_owned(),
            ..self.clone()
        })
    }

    /// The canonical resource ID, rebuilt from the parsed parts.
    pub fn id(&self) -> String {
        format!(
            "{}/providers/{}/{}/{}",
            self.resource_group_id(),
            self.kind.namespace,
            self.kind.kind,
            self.name
        )
    }

    /// The ID of a child resource, e.g. `agentPools/<name>`. `name` must be a valid name.
    pub fn child(&self, kind: &str, name: &str) -> Option<String> {
        is_name(name).then(|| format!("{}/{kind}/{name}", self.id()))
    }

    pub fn resource_group_id(&self) -> String {
        format!(
            "/subscriptions/{}/resourceGroups/{}",
            self.subscription, self.resource_group
        )
    }

    /// Whether `id` names this resource, ignoring case as ARM does.
    pub fn is(&self, id: &str) -> bool {
        id.eq_ignore_ascii_case(&self.id())
    }
}

pub fn is_guid(s: &str) -> bool {
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

/// 1-80 of ASCII letters, digits, `-`, `_`, `.`, starting with a letter or digit. This is
/// the intersection of the naming rules of the resource types above, and keeps names safe to
/// put into a URL path.
fn is_name(s: &str) -> bool {
    s.len() <= 80
        && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
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

/// Condition ARM checks before a write, so it doesn't act on a resource that changed since the
/// function read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precondition<'a> {
    Always,
    /// The resource still has this ETag; unconditional if the resource reported none.
    IfMatch(Option<&'a str>),
    /// The resource doesn't exist yet.
    IfNoneMatch,
}

impl Precondition<'_> {
    fn apply(self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            Self::Always | Self::IfMatch(None) => req,
            Self::IfMatch(Some(etag)) => req.header(reqwest::header::IF_MATCH, etag),
            Self::IfNoneMatch => req.header(reqwest::header::IF_NONE_MATCH, "*"),
        }
    }
}

/// What a function needs to reach ARM: an HTTPS client, a credential and the endpoint.
/// Created once per process; [`Connector::connect`] gets a fresh token per invocation.
#[derive(Clone)]
pub struct Connector {
    client: reqwest::Client,
    credential: Arc<dyn TokenCredential>,
    endpoint: String,
}

impl Connector {
    pub fn new(
        client: reqwest::Client,
        credential: Arc<dyn TokenCredential>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            client,
            credential,
            endpoint: endpoint.into(),
        }
    }

    /// The function app's managed identity and the public ARM endpoint, or the one in
    /// `AZURE_ARM_ENDPOINT`.
    pub fn from_env(client: reqwest::Client) -> Result<Self, Error> {
        let endpoint = std::env::var(ENDPOINT_VAR)
            .ok()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| ENDPOINT.to_owned());
        Ok(Self::new(client, crate::credential()?, endpoint))
    }

    /// Connects to ARM with a token of the credential.
    pub async fn connect(&self) -> Result<Arm, Error> {
        let token = self
            .credential
            .get_token(&[ARM_SCOPE], None)
            .await
            .map_err(provider_error)?;
        Ok(Arm {
            client: self.client.clone(),
            endpoint: self.endpoint.clone(),
            token: token.token.secret().to_owned(),
        })
    }
}

pub struct Arm {
    client: reqwest::Client,
    endpoint: String,
    token: String,
}

impl Arm {
    /// The resource at `path`, or `None` if it doesn't exist.
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        api_version: &str,
    ) -> Result<Option<T>, Error> {
        let resp = self
            .send(self.request(Method::GET, path, api_version))
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        json(resp).await.map(Some)
    }

    /// Every item of the collection at `path`, following pagination.
    pub async fn list<T: DeserializeOwned>(
        &self,
        path: &str,
        api_version: &str,
        filter: Option<&str>,
    ) -> Result<Vec<T>, Error> {
        let mut req = self.request(Method::GET, path, api_version);
        if let Some(filter) = filter {
            req = req.query(&[("$filter", filter)]);
        }
        let mut items = Vec::new();
        loop {
            let resp = self.send(req).await?;
            // The collection's parent, e.g. a deleted resource group, doesn't exist.
            if resp.status() == reqwest::StatusCode::NOT_FOUND && items.is_empty() {
                return Ok(items);
            }
            let page: List<T> = json(resp).await?;
            items.extend(page.value);
            match page.next_link {
                // Only follow links back to the same endpoint, as they carry the token.
                Some(next) if same_origin(&next, &self.endpoint) => req = self.client.get(next),
                _ => return Ok(items),
            }
        }
    }

    /// Creates or replaces the resource at `path`, if `precondition` holds.
    pub async fn put(
        &self,
        path: &str,
        api_version: &str,
        body: &Value,
        precondition: Precondition<'_>,
    ) -> Result<(), Error> {
        let req = precondition.apply(self.request(Method::PUT, path, api_version).json(body));
        ensure_success(self.send(req).await?).await
    }

    /// Updates the given properties of the resource at `path`, and returns the resource as
    /// ARM reports it after the update (`null` if it doesn't).
    pub async fn patch(
        &self,
        path: &str,
        api_version: &str,
        body: &Value,
        precondition: Precondition<'_>,
    ) -> Result<Value, Error> {
        let req = precondition.apply(self.request(Method::PATCH, path, api_version).json(body));
        let resp = self.send(req).await?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|err| Error::provider(format!("ARM: {err}")))?;
        if !status.is_success() {
            return Err(Error::provider(api_error(status.as_u16(), &text)));
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// Runs the long-running action at `path` and returns its result, polling until
    /// `deadline`. Returns `None` if the action hasn't finished by then.
    pub async fn post_and_wait(
        &self,
        path: &str,
        api_version: &str,
        deadline: tokio::time::Instant,
    ) -> Result<Option<Value>, Error> {
        let mut resp = self
            .send(self.request(Method::POST, path, api_version))
            .await?;
        loop {
            if resp.status() != reqwest::StatusCode::ACCEPTED {
                return json(resp).await.map(Some);
            }
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            // Only poll back to the same endpoint, as the requests carry the token.
            let Some(location) = location.filter(|l| same_origin(l, &self.endpoint)) else {
                return Err(Error::provider(format!(
                    "ARM: {path} was accepted without a location to poll"
                )));
            };
            let wait = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()?.parse().ok())
                .map_or(POLL_INTERVAL, |secs: u64| {
                    std::time::Duration::from_secs(secs).min(POLL_INTERVAL)
                });
            if tokio::time::Instant::now() + wait >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(wait).await;
            resp = self.send(self.client.get(location)).await?;
        }
    }

    /// Starts deleting the resource at `path`, if `precondition` holds. A resource that is
    /// already gone is not an error.
    pub async fn delete(
        &self,
        path: &str,
        api_version: &str,
        precondition: Precondition<'_>,
    ) -> Result<(), Error> {
        let req = precondition.apply(self.request(Method::DELETE, path, api_version));
        let resp = self.send(req).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        ensure_success(resp).await
    }

    /// The disk, or `None` if it doesn't exist.
    pub async fn disk(&self, id: &ResourceId) -> Result<Option<Disk>, Error> {
        self.get(&id.id(), COMPUTE_API_VERSION).await
    }

    /// Snapshots in the disk's resource group, following pagination.
    pub async fn snapshots(&self, id: &ResourceId) -> Result<Vec<Snapshot>, Error> {
        let path = format!(
            "{}/providers/Microsoft.Compute/snapshots",
            id.resource_group_id()
        );
        self.list(&path, COMPUTE_API_VERSION, None).await
    }

    /// Starts an incremental snapshot of the disk in its resource group.
    pub async fn create_snapshot(
        &self,
        disk: &ResourceId,
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
        self.put(&path, COMPUTE_API_VERSION, &body, Precondition::Always)
            .await
    }

    /// Starts deleting the disk.
    pub async fn delete_disk(&self, id: &ResourceId) -> Result<(), Error> {
        self.delete(&id.id(), COMPUTE_API_VERSION, Precondition::Always)
            .await
    }

    fn request(&self, method: Method, path: &str, api_version: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.endpoint))
            .query(&[("api-version", api_version)])
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
    fn parses_and_rebuilds_resource_ids() {
        let id = ResourceId::parse(
            &format!(
                "/subscriptions/{SUB}/resourceGroups/MC_rg_aks_eastus/providers/microsoft.compute/disks/pvc-1"
            ),
            &[DISK],
        )
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
            assert_eq!(ResourceId::parse(&id, &[DISK]), None, "{id}");
        }
    }

    #[test]
    fn accepts_only_the_expected_kinds() {
        let vm = format!(
            "/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm-1"
        );

        assert_eq!(ResourceId::parse(&vm, &[DISK]), None);
        let id = ResourceId::parse(&vm, &[DISK, VIRTUAL_MACHINE]).unwrap();
        assert_eq!(id.kind, VIRTUAL_MACHINE);
        assert!(id.is(&vm.to_uppercase()));
    }

    #[test]
    fn builds_sibling_and_child_ids() {
        let lb = ResourceId::parse(
            &format!(
                "/subscriptions/{SUB}/resourceGroups/MC_rg/providers/Microsoft.Network/loadBalancers/kubernetes"
            ),
            &[LOAD_BALANCER],
        )
        .unwrap();

        assert_eq!(
            lb.sibling(PUBLIC_IP, "kubernetes-a1").unwrap().id(),
            format!(
                "/subscriptions/{SUB}/resourceGroups/MC_rg/providers/Microsoft.Network/publicIPAddresses/kubernetes-a1"
            )
        );
        assert_eq!(
            lb.child("frontendIPConfigurations", "a1").unwrap(),
            format!("{}/frontendIPConfigurations/a1", lb.id())
        );
        for bad in ["", "..", "a/b", "a?b", "a#b", "a%2Fb", ".hidden"] {
            assert!(lb.sibling(PUBLIC_IP, bad).is_none(), "{bad}");
            assert!(lb.child("agentPools", bad).is_none(), "{bad}");
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
