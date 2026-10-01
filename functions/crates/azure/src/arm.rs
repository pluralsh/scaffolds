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

/// Wait between two polls of a long-running operation when ARM doesn't say how long to wait.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Maximum attempts for a request that ARM throttles or fails to serve.
const ATTEMPTS: u32 = 3;
/// Wait before the first retry when ARM doesn't say how long to wait; it doubles every retry.
const RETRY_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);
/// Longest wait before a retry, even if ARM's Retry-After asks for more, so retries fit in
/// the time a caller waits.
const MAX_RETRY_WAIT: std::time::Duration = std::time::Duration::from_secs(4);

/// API version of Microsoft.Compute disks and snapshots.
pub const COMPUTE_API_VERSION: &str = "2024-03-02";

/// A resource type the functions accept IDs for, in its canonical spelling.
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
    /// can't point the client at another resource or API. The ID is rebuilt from the parsed
    /// parts, and names can't contain `/`, `?`, `#`, `%` or be `.` or `..`.
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

/// 1-80 of ASCII letters, digits, `-`, `_`, `.`, starting with a letter or digit. This fits
/// the naming rules of every resource type above and keeps names safe in a URL path.
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

/// The HTTPS client, credential and endpoint for reaching ARM. Create it once per process;
/// [`Connector::connect`] gets a fresh token per invocation.
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
                None => return Ok(items),
                // Only follow links back to the same endpoint, as they carry the token. A
                // partial list must not pass for the whole one.
                Some(next) if same_origin(&next, &self.endpoint) => req = self.client.get(next),
                Some(next) => {
                    return Err(Error::provider(format!(
                        "ARM: {path} links its next page to another endpoint ({next})"
                    )));
                }
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
            // Polls when ARM asks to, and once more at the deadline if that is later.
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            tokio::time::sleep(poll_wait(resp.headers()).min(remaining)).await;
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

    /// Sends the request, retrying if ARM throttled it or failed to serve it. Timed-out
    /// requests aren't retried, so retries only add their waits to a call's duration.
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
        let mut req = req
            .bearer_auth(&self.token)
            .build()
            .map_err(|err| Error::provider(format!("ARM: {err}")))?;
        let mut attempt = 1;
        loop {
            // Bodies are JSON bytes, so requests can always be cloned.
            let retry = if attempt < ATTEMPTS {
                req.try_clone()
            } else {
                None
            };
            let resp = self
                .client
                .execute(req)
                .await
                .map_err(|err| Error::provider(format!("ARM: {err}")))?;
            match retry {
                Some(next) if retryable(next.method(), resp.status()) => {
                    let wait = retry_wait(resp.headers(), attempt);
                    tracing::warn!(
                        status = resp.status().as_u16(),
                        attempt,
                        wait_ms = wait.as_millis() as u64,
                        "retrying ARM request"
                    );
                    tokio::time::sleep(wait).await;
                    req = next;
                    attempt += 1;
                }
                _ => return Ok(resp),
            }
        }
    }
}

/// How long to wait before polling a long-running operation again: the Retry-After seconds
/// ARM asked for, or [`POLL_INTERVAL`].
fn poll_wait(headers: &reqwest::header::HeaderMap) -> std::time::Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok()?.trim().parse().ok())
        .map_or(POLL_INTERVAL, std::time::Duration::from_secs)
}

/// Whether a request that got `status` can be sent again. Throttled and unavailable requests
/// weren't processed, so any method can be retried. Other server errors may have been
/// processed, so only reads are retried. A failed write is left to the caller, who plans
/// again first.
fn retryable(method: &Method, status: reqwest::StatusCode) -> bool {
    use reqwest::StatusCode;
    match status {
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => true,
        StatusCode::INTERNAL_SERVER_ERROR
        | StatusCode::BAD_GATEWAY
        | StatusCode::GATEWAY_TIMEOUT => method == Method::GET,
        _ => false,
    }
}

/// How long to wait before retry `attempt`: the Retry-After seconds ARM asked for, or an
/// exponential backoff, at most [`MAX_RETRY_WAIT`].
fn retry_wait(headers: &reqwest::header::HeaderMap, attempt: u32) -> std::time::Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok()?.trim().parse().ok())
        .map_or(
            RETRY_BACKOFF * 2u32.pow(attempt - 1),
            std::time::Duration::from_secs,
        )
        .min(MAX_RETRY_WAIT)
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

#[cfg(test)]
mod retry_tests {
    use super::*;
    use reqwest::StatusCode;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn retries_throttled_and_unavailable_requests_of_any_method() {
        for method in [Method::GET, Method::PUT, Method::POST, Method::DELETE] {
            assert!(
                retryable(&method, StatusCode::TOO_MANY_REQUESTS),
                "{method}"
            );
            assert!(
                retryable(&method, StatusCode::SERVICE_UNAVAILABLE),
                "{method}"
            );
        }
    }

    #[test]
    fn retries_other_server_errors_of_reads_only() {
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(retryable(&Method::GET, status), "{status}");
            for method in [Method::PUT, Method::PATCH, Method::POST, Method::DELETE] {
                assert!(!retryable(&method, status), "{method} {status}");
            }
        }
        for status in [
            StatusCode::OK,
            StatusCode::ACCEPTED,
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::PRECONDITION_FAILED,
        ] {
            assert!(!retryable(&Method::GET, status), "{status}");
        }
    }

    #[test]
    fn polls_as_asked_or_every_interval() {
        let mut headers = HeaderMap::new();
        assert_eq!(poll_wait(&headers), POLL_INTERVAL);
        headers.insert(RETRY_AFTER, HeaderValue::from_static("10"));
        assert_eq!(poll_wait(&headers), std::time::Duration::from_secs(10));
    }

    #[test]
    fn waits_as_asked_up_to_a_limit_or_backs_off() {
        let after = |secs: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, HeaderValue::from_str(secs).unwrap());
            headers
        };
        let secs = std::time::Duration::from_secs;
        assert_eq!(retry_wait(&after("3"), 1), secs(3));
        assert_eq!(retry_wait(&after("0"), 2), secs(0));
        assert_eq!(retry_wait(&after("120"), 1), MAX_RETRY_WAIT);
        assert_eq!(retry_wait(&HeaderMap::new(), 1), secs(1));
        assert_eq!(retry_wait(&HeaderMap::new(), 2), secs(2));
        // A date, which ARM doesn't send, falls back to the backoff.
        assert_eq!(
            retry_wait(&after("Wed, 21 Oct 2015 07:28:00 GMT"), 1),
            secs(1)
        );
    }
}

#[cfg(all(test, feature = "mock"))]
mod mock_tests {
    use serde_json::json;

    use super::*;
    use crate::mock::MockArm;

    const PATH: &str = "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/d";
    const NOW: &[(&str, &str)] = &[("Retry-After", "0")];

    #[tokio::test]
    async fn retries_a_throttled_read_until_it_succeeds() {
        let mock = MockArm::start().await;
        mock.on_with_headers(Method::GET, PATH, 429, NOW, json!({}))
            .on_with_headers(Method::GET, PATH, 500, NOW, json!({}))
            .on(Method::GET, PATH, 200, json!({"name": "d"}));
        let arm = mock.connector().connect().await.unwrap();

        let disk: Option<Value> = arm.get(PATH, COMPUTE_API_VERSION).await.unwrap();

        assert_eq!(disk.unwrap()["name"], "d");
        assert_eq!(mock.requests().len(), 3);
    }

    #[tokio::test]
    async fn gives_up_after_the_last_attempt() {
        let mock = MockArm::start().await;
        mock.on_with_headers(Method::GET, PATH, 503, NOW, json!({}));
        let arm = mock.connector().connect().await.unwrap();

        let err = arm
            .get::<Value>(PATH, COMPUTE_API_VERSION)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("503"), "{err}");
        assert_eq!(mock.requests().len(), ATTEMPTS as usize);
    }

    #[tokio::test]
    async fn resends_a_throttled_write_with_its_body_and_precondition() {
        let mock = MockArm::start().await;
        mock.on_with_headers(Method::PUT, PATH, 429, NOW, json!({}))
            .on(Method::PUT, PATH, 200, json!({}));
        let arm = mock.connector().connect().await.unwrap();

        arm.put(
            PATH,
            COMPUTE_API_VERSION,
            &json!({"location": "eastus"}),
            Precondition::IfMatch(Some("\"etag-1\"")),
        )
        .await
        .unwrap();

        let writes = mock.writes();
        assert_eq!(writes.len(), 2);
        for write in writes {
            assert_eq!(write.body, json!({"location": "eastus"}));
            assert_eq!(write.if_match.as_deref(), Some("\"etag-1\""));
        }
    }

    #[tokio::test]
    async fn does_not_resend_a_write_that_failed_on_the_server() {
        let mock = MockArm::start().await;
        mock.on_with_headers(Method::PUT, PATH, 500, NOW, json!({}))
            .on(Method::PUT, PATH, 200, json!({}));
        let arm = mock.connector().connect().await.unwrap();

        let result = arm
            .put(PATH, COMPUTE_API_VERSION, &json!({}), Precondition::Always)
            .await;

        assert!(result.is_err());
        assert_eq!(mock.writes().len(), 1);
    }

    #[tokio::test]
    async fn follows_next_links_to_the_end() {
        let mock = MockArm::start().await;
        let list = "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks";
        mock.on(
            Method::GET,
            list,
            200,
            json!({"value": [{"name": "a"}], "nextLink": format!("{}{list}?$skiptoken=2", mock.url())}),
        )
        .on(Method::GET, list, 200, json!({"value": [{"name": "b"}]}));
        let arm = mock.connector().connect().await.unwrap();

        let items: Vec<Value> = arm.list(list, COMPUTE_API_VERSION, None).await.unwrap();

        assert_eq!(items, vec![json!({"name": "a"}), json!({"name": "b"})]);
        assert!(mock.requests()[1].query.contains("skiptoken=2"));
    }

    #[tokio::test]
    async fn refuses_next_links_to_another_endpoint() {
        let mock = MockArm::start().await;
        let list = "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks";
        mock.on(
            Method::GET,
            list,
            200,
            json!({"value": [{"name": "a"}], "nextLink": "https://example.com/next"}),
        );
        let arm = mock.connector().connect().await.unwrap();

        let err = arm
            .list::<Value>(list, COMPUTE_API_VERSION, None)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("another endpoint"), "{err}");
        assert_eq!(mock.requests().len(), 1);
    }

    #[tokio::test]
    async fn reports_an_etag_conflict_without_resending() {
        let mock = MockArm::start().await;
        mock.on(
            Method::PUT,
            PATH,
            412,
            json!({"error": {"code": "PreconditionFailed", "message": "The ETag doesn't match."}}),
        );
        let arm = mock.connector().connect().await.unwrap();

        let err = arm
            .put(
                PATH,
                COMPUTE_API_VERSION,
                &json!({}),
                Precondition::IfMatch(Some("\"stale\"")),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("PreconditionFailed"), "{err}");
        assert_eq!(mock.writes().len(), 1);
    }

    const OPERATION: &str =
        "/subscriptions/s/providers/Microsoft.Network/locations/eastus/operationResults/op";

    #[tokio::test]
    async fn polls_a_long_running_action_for_its_result() {
        let mock = MockArm::start().await;
        let location = format!("{}{OPERATION}", mock.url());
        let action = format!("{PATH}/health");
        mock.on_with_headers(
            Method::POST,
            &action,
            202,
            &[("Location", &location), ("Retry-After", "0")],
            json!({}),
        )
        .on_with_headers(
            Method::GET,
            OPERATION,
            202,
            &[("Location", &location), ("Retry-After", "0")],
            json!({}),
        )
        .on(Method::GET, OPERATION, 200, json!({"up": 1}));
        let arm = mock.connector().connect().await.unwrap();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

        let result = arm
            .post_and_wait(&action, COMPUTE_API_VERSION, deadline)
            .await
            .unwrap();

        assert_eq!(result, Some(json!({"up": 1})));
        assert_eq!(mock.requests().len(), 3);
    }

    #[tokio::test]
    async fn polls_once_more_at_the_deadline_when_asked_to_wait_longer() {
        let mock = MockArm::start().await;
        let location = format!("{}{OPERATION}", mock.url());
        let action = format!("{PATH}/health");
        mock.on_with_headers(
            Method::POST,
            &action,
            202,
            &[("Location", &location), ("Retry-After", "30")],
            json!({}),
        )
        .on_with_headers(
            Method::GET,
            OPERATION,
            202,
            &[("Location", &location), ("Retry-After", "30")],
            json!({}),
        );
        let arm = mock.connector().connect().await.unwrap();
        let started = tokio::time::Instant::now();
        let deadline = started + std::time::Duration::from_millis(300);

        let result = arm
            .post_and_wait(&action, COMPUTE_API_VERSION, deadline)
            .await
            .unwrap();

        assert_eq!(result, None);
        assert_eq!(mock.requests().len(), 2);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
