//! Sets the node count of a manually scaled AKS node pool.
//!
//! AKS cordons and drains the nodes it removes when scaling down. Pools scaled by the cluster
//! autoscaler are refused, since the autoscaler owns their count; change its minimum and
//! maximum instead. The new count is bounded by the pool mode (system pools keep at least one
//! node) and by the installation's `MAX_NODE_COUNT`.

use std::sync::Arc;

use functions_azure::ManagedIdentityCredential;
use functions_azure::arm::{self, Arm, Precondition, ResourceId};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const AKS_API_VERSION: &str = "2026-07-01";

/// Nodes AKS allows in a single node pool.
const AKS_MAX_COUNT: i64 = 1000;

/// Environment variable with the largest count callers may set, at most [`AKS_MAX_COUNT`].
const MAX_COUNT_VAR: &str = "MAX_NODE_COUNT";

/// Agent pool properties AKS computes itself, left out of the update.
const READ_ONLY_PROPERTIES: [&str; 4] = [
    "provisioningState",
    "currentOrchestratorVersion",
    "nodeImageVersion",
    "eTag",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    cluster_id: String,
    node_pool: String,
    count: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgentPool {
    #[serde(default)]
    name: String,
    #[serde(default)]
    properties: PoolProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoolProperties {
    /// Sent back as `If-Match`, so a pool changed since it was read isn't overwritten.
    #[serde(default, rename = "eTag")]
    etag: Option<String>,
    #[serde(default)]
    count: Option<i64>,
    #[serde(default)]
    enable_auto_scaling: Option<bool>,
    #[serde(default)]
    min_count: Option<i64>,
    #[serde(default)]
    max_count: Option<i64>,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    provisioning_state: String,
    #[serde(default)]
    power_state: Option<PowerState>,
    #[serde(default)]
    vm_size: Option<String>,
    /// `VirtualMachineScaleSets` (the default) or `VirtualMachines`, which scale differently.
    #[serde(default, rename = "type")]
    pool_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PowerState {
    #[serde(default)]
    code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NodePool {
    name: String,
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vm_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<i64>,
    state: String,
    autoscaling: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    node_pool: Option<NodePool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<i64>,
    to: i64,
    /// Whether the new count was submitted. AKS then adds or drains nodes in the background.
    submitted: bool,
}

async fn handle(
    client: &reqwest::Client,
    credential: &ManagedIdentityCredential,
    req: Request<Params>,
) -> Result<Response<Output>, Error> {
    let params = req.params;
    let cluster = ResourceId::parse(&params.cluster_id, &[arm::MANAGED_CLUSTER]).ok_or_else(|| {
        Error::invalid_request(format!(
            "clusterId {:?} is not an AKS cluster resource ID (/subscriptions/<id>/resourceGroups/<group>/providers/Microsoft.ContainerService/managedClusters/<name>)",
            params.cluster_id
        ))
    })?;
    let path = cluster
        .child("agentPools", &params.node_pool)
        .ok_or_else(|| {
            Error::invalid_request(format!(
                "nodePool {:?} is not a node pool name",
                params.node_pool
            ))
        })?;
    if params.count < 0 {
        return Err(Error::invalid_request("count can't be negative"));
    }

    let arm = Arm::connect(client.clone(), credential).await?;
    let raw: Option<Value> = arm.get(&path, AKS_API_VERSION).await?;
    let pool = raw
        .as_ref()
        .map(|v| serde_json::from_value::<AgentPool>(v.clone()))
        .transpose()
        .map_err(|err| Error::provider(format!("ARM response: {err}")))?;
    let guards = evaluate(pool.as_ref(), params.count, max_count());
    let mut output = Output {
        node_pool: pool.as_ref().map(node_pool),
        from: pool.as_ref().and_then(|p| p.properties.count),
        to: params.count,
        submitted: false,
    };

    match (req.action, raw, pool) {
        (Action::Plan, ..) => Ok(Response::planned(guards, output)),
        (Action::Execute, Some(raw), Some(pool)) if guards.iter().all(|g| g.passed) => {
            if output.from == Some(params.count) {
                return Ok(Response::done(guards, output));
            }
            arm.put(
                &path,
                AKS_API_VERSION,
                &update_body(&raw, params.count),
                Precondition::IfMatch(pool.properties.etag.as_deref()),
            )
            .await?;
            tracing::info!(pool = %path, from = ?output.from, to = params.count, "scaling node pool");
            output.submitted = true;
            Ok(Response::done(guards, output))
        }
        (Action::Execute, ..) => Ok(Response::refused(guards).with_result(output)),
    }
}

/// The largest count the installation allows.
fn max_count() -> i64 {
    cap(std::env::var(MAX_COUNT_VAR).ok().as_deref())
}

fn cap(value: Option<&str>) -> i64 {
    value
        .and_then(|v| v.parse().ok())
        .map_or(AKS_MAX_COUNT, |max: i64| max.min(AKS_MAX_COUNT))
}

fn evaluate(pool: Option<&AgentPool>, count: i64, max: i64) -> Vec<Guard> {
    let Some(pool) = pool else {
        return vec![Guard::fail("exists", "node pool not found")];
    };
    let props = &pool.properties;
    let power = props
        .power_state
        .as_ref()
        .map_or("Running", |p| p.code.as_str());
    let min = if props.mode.eq_ignore_ascii_case("System") {
        1
    } else {
        0
    };
    vec![
        Guard::pass("exists", format!("node pool {}", pool.name)),
        Guard::check(
            "idle",
            props.provisioning_state == "Succeeded",
            format!(
                "provisioning state {}; another operation must finish first unless Succeeded",
                props.provisioning_state
            ),
        ),
        Guard::check(
            "running",
            power == "Running",
            format!("power state {power}"),
        ),
        {
            let kind = props
                .pool_type
                .as_deref()
                .unwrap_or("VirtualMachineScaleSets");
            Guard::check(
                "scale-set-pool",
                kind == "VirtualMachineScaleSets",
                format!("{kind} pool; only scale set pools are scaled by their count"),
            )
        },
        match props.enable_auto_scaling {
            Some(true) => Guard::fail(
                "manually-scaled",
                format!(
                    "the cluster autoscaler scales this pool between {} and {} nodes; change those instead",
                    props.min_count.map_or("?".into(), |c| c.to_string()),
                    props.max_count.map_or("?".into(), |c| c.to_string())
                ),
            ),
            _ => Guard::pass("manually-scaled", "cluster autoscaler disabled"),
        },
        Guard::check(
            "count-allowed",
            (min..=max).contains(&count),
            format!("{count} nodes; {} pools allow {min} to {max}", props.mode),
        ),
    ]
}

fn node_pool(pool: &AgentPool) -> NodePool {
    let props = &pool.properties;
    NodePool {
        name: pool.name.clone(),
        mode: props.mode.clone(),
        vm_size: props.vm_size.clone(),
        count: props.count,
        state: props.provisioning_state.clone(),
        autoscaling: props.enable_auto_scaling.unwrap_or(false),
    }
}

/// The agent pool as read, with the new count and without the properties AKS computes.
fn update_body(raw: &Value, count: i64) -> Value {
    let mut props = raw
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in READ_ONLY_PROPERTIES {
        props.remove(key);
    }
    props.insert("count".into(), count.into());
    serde_json::json!({ "properties": props })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = functions_http::https_client().map_err(std::io::Error::other)?;
    let credential: Arc<ManagedIdentityCredential> =
        functions_azure::credential().map_err(std::io::Error::other)?;

    functions_http::run(move |req| {
        let client = client.clone();
        let credential = credential.clone();
        async move { handle(&client, &credential, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pool(value: Value) -> AgentPool {
        serde_json::from_value(value).unwrap()
    }

    fn user_pool() -> Value {
        json!({
            "name": "apps",
            "properties": {
                "eTag": "\"1\"",
                "count": 3, "mode": "User", "vmSize": "Standard_D4s_v5",
                "enableAutoScaling": false, "provisioningState": "Succeeded",
                "powerState": {"code": "Running"},
                "currentOrchestratorVersion": "1.33.2", "nodeImageVersion": "AKSUbuntu-2204",
                "orchestratorVersion": "1.33", "nodeLabels": {"team": "a"}
            }
        })
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    #[test]
    fn allows_resizing_an_idle_manual_pool() {
        let p = pool(user_pool());

        assert!(failed(&evaluate(Some(&p), 0, 10)).is_empty());
        assert!(failed(&evaluate(Some(&p), 10, 10)).is_empty());
        assert_eq!(failed(&evaluate(Some(&p), 11, 10)), ["count-allowed"]);
    }

    #[test]
    fn keeps_a_node_in_system_pools() {
        let mut v = user_pool();
        v["properties"]["mode"] = json!("System");

        assert_eq!(failed(&evaluate(Some(&pool(v)), 0, 10)), ["count-allowed"]);
    }

    #[test]
    fn refuses_autoscaled_busy_and_stopped_pools() {
        let mut v = user_pool();
        v["properties"]["enableAutoScaling"] = json!(true);
        v["properties"]["provisioningState"] = json!("Scaling");
        v["properties"]["powerState"]["code"] = json!("Stopped");

        assert_eq!(
            failed(&evaluate(Some(&pool(v)), 2, 10)),
            ["idle", "running", "manually-scaled"]
        );
        assert_eq!(failed(&evaluate(None, 2, 10)), ["exists"]);
    }

    #[test]
    fn refuses_virtual_machines_pools() {
        let mut v = user_pool();
        v["properties"]["type"] = json!("VirtualMachines");

        assert_eq!(failed(&evaluate(Some(&pool(v)), 2, 10)), ["scale-set-pool"]);
    }

    #[test]
    fn update_keeps_the_pool_and_changes_only_the_count() {
        let body = update_body(&user_pool(), 5);
        let props = &body["properties"];

        assert_eq!(props["count"], 5);
        assert_eq!(props["orchestratorVersion"], "1.33");
        assert_eq!(props["nodeLabels"], json!({"team": "a"}));
        for key in READ_ONLY_PROPERTIES {
            assert!(props.get(key).is_none(), "{key}");
        }
        assert!(body.get("etag").is_none() && body.get("name").is_none());
        assert_eq!(pool(user_pool()).properties.etag.as_deref(), Some("\"1\""));
    }

    #[test]
    fn caps_the_count_at_the_aks_limit() {
        assert_eq!(cap(None), AKS_MAX_COUNT);
        assert_eq!(cap(Some("5000")), AKS_MAX_COUNT);
        assert_eq!(cap(Some("20")), 20);
        assert_eq!(cap(Some("")), AKS_MAX_COUNT);
    }
}
