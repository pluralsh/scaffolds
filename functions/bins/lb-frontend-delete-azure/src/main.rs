//! Removes what a deleted Kubernetes `LoadBalancer` Service left on an AKS load balancer.
//!
//! AKS puts every such Service on a shared load balancer (`kubernetes` or
//! `kubernetes-internal`) as a frontend IP configuration named after the Service UID (`a` and
//! the first 31 hex digits of the UID), with load balancing rules and health probes named
//! after the frontend and, for public Services, a public IP tagged with the Service. The
//! caller confirms that no Service with that UID exists anymore, and the frontend's rules must
//! have no healthy backends; the function then removes, one execute at a time:
//!
//! 1. the frontend, its rules and the probes only those rules used, with the load balancer's
//!    ETag so a concurrent update by the cloud provider is never overwritten; the whole load
//!    balancer is deleted instead when this was its last frontend and it has no backends,
//! 2. the Service's public IP, once the load balancer no longer uses it, if AKS created it
//!    for that Service and no other.
//!
//! The last frontend of a load balancer that still has backends is refused: AKS deletes such
//! a load balancer itself once it has taken the nodes out of its backend pools.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use functions_azure::ManagedIdentityCredential;
use functions_azure::arm::{self, Arm, Precondition, ResourceId};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const NETWORK_API_VERSION: &str = "2024-05-01";

/// Tag the cloud provider sets on public IPs it creates, listing the Services using them.
const SERVICE_TAG: &str = "k8s-azure-service";

/// Prefix of the public IPs the cloud provider creates, followed by the frontend name.
const PUBLIC_IP_PREFIX: &str = "kubernetes-";

/// Hex digits of the Service UID in a frontend name, which the cloud provider cuts at 32 characters.
const UID_DIGITS: usize = 31;

/// How long to wait for the backend health of the frontend's rules, within an invocation.
const HEALTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    load_balancer_id: String,
    frontend_name: String,
    /// `namespace/name` of the deleted Service.
    service_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LoadBalancer {
    #[serde(default)]
    name: String,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    properties: LbProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LbProperties {
    #[serde(default)]
    provisioning_state: String,
    #[serde(default, rename = "frontendIPConfigurations")]
    frontends: Vec<Frontend>,
    #[serde(default)]
    backend_address_pools: Vec<BackendPool>,
    #[serde(default)]
    load_balancing_rules: Vec<Named<RuleProperties>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Named<P> {
    #[serde(default)]
    name: String,
    #[serde(default)]
    properties: P,
}

type Frontend = Named<FrontendProperties>;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendProperties {
    #[serde(default)]
    public_ip_address: Option<SubResource>,
    #[serde(default)]
    private_ip_address: Option<String>,
    #[serde(default)]
    outbound_rules: Vec<SubResource>,
    #[serde(default)]
    inbound_nat_rules: Vec<SubResource>,
    #[serde(default)]
    inbound_nat_pools: Vec<SubResource>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleProperties {
    #[serde(default, rename = "frontendIPConfiguration")]
    frontend: Option<SubResource>,
    #[serde(default)]
    probe: Option<SubResource>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendPool {
    #[serde(default)]
    properties: BackendPoolProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendPoolProperties {
    #[serde(default, rename = "backendIPConfigurations")]
    ip_configurations: Vec<SubResource>,
    #[serde(default)]
    load_balancer_backend_addresses: Vec<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SubResource {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicIp {
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    properties: PublicIpProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicIpProperties {
    #[serde(default)]
    ip_address: Option<String>,
    #[serde(default)]
    ip_configuration: Option<SubResource>,
}

/// The single change an execute makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Step {
    RemoveFrontend,
    DeleteLoadBalancer,
    DeletePublicIp,
    Nothing,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontendSummary {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_ip: Option<String>,
    rules: Vec<String>,
    probes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    frontend: Option<FrontendSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_ip: Option<String>,
    /// What the next execute does, or did for a done execute.
    step: Step,
    /// Whether anything is left for a later execute.
    remaining: bool,
}

async fn handle(
    client: &reqwest::Client,
    credential: &ManagedIdentityCredential,
    req: Request<Params>,
) -> Result<Response<Output>, Error> {
    let params = req.params;
    let lb_id = ResourceId::parse(&params.load_balancer_id, &[arm::LOAD_BALANCER]).ok_or_else(|| {
        Error::invalid_request(format!(
            "loadBalancerId {:?} is not a load balancer resource ID (/subscriptions/<id>/resourceGroups/<group>/providers/Microsoft.Network/loadBalancers/<name>)",
            params.load_balancer_id
        ))
    })?;
    if !is_service_frontend(&params.frontend_name) {
        return Err(Error::invalid_request(format!(
            "frontendName {:?} is not a Service frontend (a<Service UID without dashes>, e.g. a1b2c3...); only those can be removed",
            params.frontend_name
        )));
    }
    if !is_service_name(&params.service_name) {
        return Err(Error::invalid_request(format!(
            "serviceName {:?} is not <namespace>/<name>",
            params.service_name
        )));
    }
    let pip_id = lb_id
        .sibling(
            arm::PUBLIC_IP,
            &format!("{PUBLIC_IP_PREFIX}{}", params.frontend_name),
        )
        .ok_or_else(|| Error::invalid_request("frontendName is too long"))?;

    let arm = Arm::connect(client.clone(), credential).await?;
    let raw: Option<Value> = arm.get(&lb_id.id(), NETWORK_API_VERSION).await?;
    let lb = raw
        .as_ref()
        .map(|v| serde_json::from_value::<LoadBalancer>(v.clone()))
        .transpose()
        .map_err(|err| Error::provider(format!("ARM response: {err}")))?;
    let pip: Option<PublicIp> = arm.get(&pip_id.id(), NETWORK_API_VERSION).await?;
    let health = match &lb {
        Some(lb) => rule_health(&arm, lb, &lb_id, &params.frontend_name).await?,
        None => HashMap::new(),
    };

    let plan = evaluate(
        lb.as_ref(),
        &lb_id,
        pip.as_ref(),
        &pip_id,
        &params.frontend_name,
        &params.service_name,
        &health,
    );
    let output = Output {
        frontend: lb
            .as_ref()
            .and_then(|lb| frontend_summary(lb, &lb_id, &params.frontend_name)),
        public_ip: pip.as_ref().map(|_| pip_id.name.clone()),
        step: plan.step,
        remaining: plan.remaining,
    };
    let safe = plan.guards.iter().all(|g| g.passed);

    match (req.action, plan.step) {
        (Action::Plan, _) => Ok(Response::planned(plan.guards, output)),
        (Action::Execute, _) if !safe => Ok(Response::refused(plan.guards).with_result(output)),
        (Action::Execute, Step::Nothing) => Ok(Response::done(plan.guards, output)),
        (Action::Execute, Step::RemoveFrontend) => {
            // The step is only chosen for a frontend of an existing load balancer.
            let (Some(raw), Some(lb)) = (raw, lb) else {
                return Err(Error::provider("load balancer not found"));
            };
            let body = without_frontend(&raw, &lb, &lb_id, &params.frontend_name);
            arm.put(
                &lb_id.id(),
                NETWORK_API_VERSION,
                &body,
                Precondition::IfMatch(lb.etag.as_deref()),
            )
            .await?;
            tracing::info!(lb = %lb_id.id(), frontend = %params.frontend_name, "removed frontend");
            Ok(Response::done(plan.guards, output))
        }
        (Action::Execute, Step::DeleteLoadBalancer) => {
            let etag = lb.as_ref().and_then(|lb| lb.etag.as_deref());
            arm.delete(
                &lb_id.id(),
                NETWORK_API_VERSION,
                Precondition::IfMatch(etag),
            )
            .await?;
            tracing::info!(lb = %lb_id.id(), "deleting load balancer");
            Ok(Response::done(plan.guards, output))
        }
        (Action::Execute, Step::DeletePublicIp) => {
            let etag = pip.as_ref().and_then(|p| p.etag.as_deref());
            arm.delete(
                &pip_id.id(),
                NETWORK_API_VERSION,
                Precondition::IfMatch(etag),
            )
            .await?;
            tracing::info!(public_ip = %pip_id.id(), "deleting public IP");
            Ok(Response::done(plan.guards, output))
        }
    }
}

/// `a` followed by the first 31 hex digits of a Service UID without dashes, optionally with a
/// suffix such as `-IPv6` or a subnet name.
fn is_service_frontend(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('a') else {
        return false;
    };
    let (uid, suffix) = rest.split_at(rest.len().min(UID_DIGITS));
    uid.len() == UID_DIGITS
        && uid
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        && (suffix.is_empty()
            || (suffix.len() > 1
                && suffix.starts_with('-')
                && suffix[1..]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))))
}

/// Healthy backends per rule of the frontend, `None` for a rule whose health wasn't reported
/// in time.
async fn rule_health(
    arm: &Arm,
    lb: &LoadBalancer,
    lb_id: &ResourceId,
    frontend: &str,
) -> Result<HashMap<String, Option<i64>>, Error> {
    #[derive(Deserialize)]
    // A result without `up` counts as unknown, which refuses, rather than as no healthy backends.
    struct Health {
        #[serde(default)]
        up: Option<i64>,
    }

    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    let (rules, _) = owned_by_frontend(lb, lb_id, frontend);
    let mut health = HashMap::new();
    for rule in rules {
        let Some(path) = lb_id.child("loadBalancingRules", &rule) else {
            health.insert(rule, None);
            continue;
        };
        let up = arm
            .post_and_wait(&format!("{path}/health"), NETWORK_API_VERSION, deadline)
            .await?
            .map(|v| serde_json::from_value::<Health>(v).map(|h| h.up))
            .transpose()
            .map_err(|err| Error::provider(format!("ARM response: {err}")))?
            .flatten();
        health.insert(rule, up);
    }
    Ok(health)
}

/// `<namespace>/<name>`, both DNS labels.
fn is_service_name(name: &str) -> bool {
    let label = |s: &str| {
        (1..=63).contains(&s.len())
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !s.starts_with('-')
            && !s.ends_with('-')
    };
    matches!(name.split_once('/'), Some((ns, svc)) if label(ns) && label(svc))
}

struct Plan {
    guards: Vec<Guard>,
    step: Step,
    remaining: bool,
}

fn evaluate(
    lb: Option<&LoadBalancer>,
    lb_id: &ResourceId,
    pip: Option<&PublicIp>,
    pip_id: &ResourceId,
    frontend_name: &str,
    service: &str,
    health: &HashMap<String, Option<i64>>,
) -> Plan {
    let mut guards = vec![Guard::pass(
        "service-frontend",
        format!("{frontend_name} is named after a Service UID; confirm no Service with it exists"),
    )];
    let frontend = lb.and_then(|lb| {
        lb.properties
            .frontends
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(frontend_name))
    });

    // The public IP is only deleted if AKS created it for this Service alone.
    let owner_guard = |pip: &PublicIp| {
        let services: Vec<&str> = pip
            .tags
            .get(SERVICE_TAG)
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        match services.as_slice() {
            [only] if *only == service => Guard::pass(
                "public-ip-owner",
                format!("{} was created for {service}", pip_id.name),
            ),
            [] => Guard::fail(
                "public-ip-owner",
                format!("{} has no {SERVICE_TAG} tag", pip_id.name),
            ),
            other => Guard::fail(
                "public-ip-owner",
                format!(
                    "{} is used by {}, not only {service}",
                    pip_id.name,
                    other.join(", ")
                ),
            ),
        }
    };

    let step = match (lb, frontend) {
        (Some(lb), Some(frontend)) => {
            let props = &frontend.properties;
            guards.push(Guard::check(
                "idle",
                lb.properties.provisioning_state == "Succeeded",
                format!(
                    "load balancer {} is {}",
                    lb.name, lb.properties.provisioning_state
                ),
            ));
            guards.push(Guard::check(
                "inbound-only",
                props.outbound_rules.is_empty()
                    && props.inbound_nat_rules.is_empty()
                    && props.inbound_nat_pools.is_empty(),
                "the frontend must not be used by outbound or NAT rules",
            ));
            guards.push(health_guard(health));
            // Services sharing an IP share its frontend, which is named after the first of
            // them; their rules are named after each Service.
            let prefix = frontend_name.to_lowercase();
            let mut foreign: Vec<String> = owned_by_frontend(lb, lb_id, frontend_name)
                .0
                .into_iter()
                .filter(|rule| !rule.starts_with(&prefix))
                .collect();
            foreign.sort();
            guards.push(if foreign.is_empty() {
                Guard::pass(
                    "single-service",
                    "every rule belongs to this frontend's Service",
                )
            } else {
                Guard::fail(
                    "single-service",
                    format!(
                        "rules {} belong to other Services sharing the frontend",
                        foreign.join(", ")
                    ),
                )
            });
            match &props.public_ip_address {
                Some(ip) if pip_id.is(&ip.id) => {
                    if let Some(pip) = pip {
                        guards.push(owner_guard(pip));
                    }
                }
                Some(ip) => guards.push(Guard::pass(
                    "public-ip-owner",
                    format!(
                        "uses public IP {}, which AKS didn't create for it; it is kept",
                        ip.id.rsplit('/').next().unwrap_or(&ip.id)
                    ),
                )),
                None => guards.push(Guard::pass(
                    "public-ip-owner",
                    "internal frontend without a public IP",
                )),
            }
            let last = lb.properties.frontends.len() == 1;
            let members = lb.properties.backend_address_pools.iter().any(|p| {
                !p.properties.ip_configurations.is_empty()
                    || !p.properties.load_balancer_backend_addresses.is_empty()
            });
            if last && members {
                guards.push(Guard::fail(
                    "not-last-frontend",
                    format!(
                        "this is the last frontend of {} and its backend pools still have members; AKS deletes the load balancer itself once it has removed the nodes from them",
                        lb.name
                    ),
                ));
            }
            if last && !members {
                Step::DeleteLoadBalancer
            } else {
                Step::RemoveFrontend
            }
        }
        _ => match pip {
            Some(pip) => {
                guards.push(owner_guard(pip));
                guards.push(Guard::check(
                    "public-ip-unused",
                    pip.properties.ip_configuration.is_none(),
                    match &pip.properties.ip_configuration {
                        Some(cfg) => format!(
                            "still used by {}; execute again once the load balancer update has finished",
                            cfg.id
                        ),
                        None => format!(
                            "{} ({}) is not in use",
                            pip_id.name,
                            pip.properties.ip_address.as_deref().unwrap_or("no address")
                        ),
                    },
                ));
                Step::DeletePublicIp
            }
            None => {
                guards.push(Guard::pass(
                    "removed",
                    format!(
                        "neither the frontend on {} nor public IP {} exists",
                        lb_id.name, pip_id.name
                    ),
                ));
                Step::Nothing
            }
        },
    };
    let remaining = match step {
        Step::RemoveFrontend | Step::DeleteLoadBalancer => pip.is_some(),
        Step::DeletePublicIp | Step::Nothing => false,
    };
    Plan {
        guards,
        step,
        remaining,
    }
}

fn health_guard(health: &HashMap<String, Option<i64>>) -> Guard {
    let mut rules: Vec<(&String, &Option<i64>)> = health.iter().collect();
    rules.sort();
    if rules.is_empty() {
        return Guard::pass(
            "no-healthy-backends",
            "the frontend has no load balancing rules",
        );
    }
    let healthy: Vec<String> = rules
        .iter()
        .filter_map(|(rule, up)| up.filter(|n| *n > 0).map(|n| format!("{rule} ({n})")))
        .collect();
    let unknown: Vec<&str> = rules
        .iter()
        .filter(|(_, up)| up.is_none())
        .map(|(rule, _)| rule.as_str())
        .collect();
    if !healthy.is_empty() {
        Guard::fail(
            "no-healthy-backends",
            format!(
                "healthy backends on {}; the Service may still exist",
                healthy.join(", ")
            ),
        )
    } else if !unknown.is_empty() {
        Guard::fail(
            "no-healthy-backends",
            format!(
                "backend health of {} wasn't reported in time; try again",
                unknown.join(", ")
            ),
        )
    } else {
        Guard::pass(
            "no-healthy-backends",
            format!(
                "no healthy backends on {}",
                rules
                    .iter()
                    .map(|(r, _)| r.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    }
}

/// Names of the rules using the frontend and of the probes only those rules use.
fn owned_by_frontend(
    lb: &LoadBalancer,
    lb_id: &ResourceId,
    frontend: &str,
) -> (HashSet<String>, HashSet<String>) {
    let frontend_id = lb_id.child("frontendIPConfigurations", frontend);
    let uses_frontend = |rule: &Named<RuleProperties>| {
        rule.properties.frontend.as_ref().is_some_and(|f| {
            frontend_id
                .as_deref()
                .is_some_and(|id| f.id.eq_ignore_ascii_case(id))
        })
    };
    let probe_name = |rule: &Named<RuleProperties>| {
        rule.properties
            .probe
            .as_ref()
            .map(|p| p.id.rsplit('/').next().unwrap_or(&p.id).to_lowercase())
    };
    let (removed, kept): (Vec<_>, Vec<_>) = lb
        .properties
        .load_balancing_rules
        .iter()
        .partition(|r| uses_frontend(r));
    let kept_probes: HashSet<String> = kept.iter().filter_map(|r| probe_name(r)).collect();
    let probes = removed
        .iter()
        .filter_map(|r| probe_name(r))
        .filter(|p| !kept_probes.contains(p))
        .collect();
    let rules = removed.iter().map(|r| r.name.to_lowercase()).collect();
    (rules, probes)
}

/// The load balancer as read, without the frontend, its rules and the probes only they used.
fn without_frontend(raw: &Value, lb: &LoadBalancer, lb_id: &ResourceId, frontend: &str) -> Value {
    let (rules, probes) = owned_by_frontend(lb, lb_id, frontend);
    let mut body = raw.clone();
    if let Some(obj) = body.as_object_mut() {
        obj.remove("etag");
    }
    let mut retain = |key: &str, drop: &dyn Fn(&str) -> bool| {
        if let Some(items) = body
            .pointer_mut(&format!("/properties/{key}"))
            .and_then(Value::as_array_mut)
        {
            items.retain(|item| {
                !item
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| drop(&name.to_lowercase()))
            });
        }
    };
    let frontend = frontend.to_lowercase();
    retain("frontendIPConfigurations", &|name| name == frontend);
    retain("loadBalancingRules", &|name| rules.contains(name));
    retain("probes", &|name| probes.contains(name));
    body
}

fn frontend_summary(lb: &LoadBalancer, lb_id: &ResourceId, name: &str) -> Option<FrontendSummary> {
    let frontend = lb
        .properties
        .frontends
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))?;
    let (rules, probes) = owned_by_frontend(lb, lb_id, name);
    let mut rules: Vec<String> = rules.into_iter().collect();
    let mut probes: Vec<String> = probes.into_iter().collect();
    rules.sort();
    probes.sort();
    Some(FrontendSummary {
        name: frontend.name.clone(),
        private_ip: frontend.properties.private_ip_address.clone(),
        rules,
        probes,
    })
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

    const SUB: &str = "00000000-0000-0000-0000-000000000000";
    const FE: &str = "a0123456789abcdef0123456789abcde";
    const OTHER: &str = "afedcba9876543210fedcba987654321";
    const SERVICE: &str = "apps/web";

    fn lb_id() -> ResourceId {
        ResourceId::parse(
            &format!("/subscriptions/{SUB}/resourceGroups/MC_rg/providers/Microsoft.Network/loadBalancers/kubernetes"),
            &[arm::LOAD_BALANCER],
        )
        .unwrap()
    }

    fn pip_id() -> ResourceId {
        lb_id()
            .sibling(arm::PUBLIC_IP, &format!("kubernetes-{FE}"))
            .unwrap()
    }

    fn sub(kind: &str, name: &str) -> Value {
        json!({"id": format!("{}/{kind}/{name}", lb_id().id())})
    }

    fn rule(name: &str, frontend: &str, probe: &str) -> Value {
        json!({"name": name, "properties": {
            "frontendIPConfiguration": sub("frontendIPConfigurations", frontend),
            "probe": sub("probes", probe),
        }})
    }

    fn raw_lb(members: bool) -> Value {
        json!({
            "name": "kubernetes",
            "etag": "W/\"1\"",
            "location": "eastus",
            "properties": {
                "provisioningState": "Succeeded",
                "frontendIPConfigurations": [
                    {"name": FE, "properties": {"publicIPAddress": {"id": pip_id().id()}}},
                    {"name": OTHER, "properties": {"publicIPAddress": {"id": "/x/publicIPAddresses/other"}}},
                ],
                "backendAddressPools": [{"name": "kubernetes", "properties": {
                    "backendIPConfigurations": if members { json!([{"id": "/x/ipConfigurations/node-1"}]) } else { json!([]) }
                }}],
                "loadBalancingRules": [
                    rule(&format!("{FE}-TCP-80"), FE, &format!("{FE}-TCP-80")),
                    rule(&format!("{FE}-TCP-443"), FE, "shared"),
                    rule(&format!("{OTHER}-TCP-80"), OTHER, "shared"),
                ],
                "probes": [{"name": format!("{FE}-TCP-80")}, {"name": "shared"}],
            }
        })
    }

    fn lb(raw: &Value) -> LoadBalancer {
        serde_json::from_value(raw.clone()).unwrap()
    }

    fn pip(tag: Option<&str>, used: bool) -> PublicIp {
        serde_json::from_value(json!({
            "tags": tag.map(|t| json!({SERVICE_TAG: t})).unwrap_or(json!({})),
            "properties": {"ipAddress": "20.0.0.1",
                           "ipConfiguration": if used { json!({"id": "/x/frontendIPConfigurations/fe"}) } else { Value::Null }}
        }))
        .unwrap()
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    fn plan(lb: Option<&LoadBalancer>, pip: Option<&PublicIp>) -> Plan {
        evaluate(lb, &lb_id(), pip, &pip_id(), FE, SERVICE, &down())
    }

    /// Health of the frontend's rules in raw_lb, with no healthy backends.
    fn down() -> HashMap<String, Option<i64>> {
        HashMap::from([
            (format!("{FE}-tcp-80"), Some(0)),
            (format!("{FE}-tcp-443"), Some(0)),
        ])
    }

    #[test]
    fn accepts_only_service_frontends_and_names() {
        assert!(is_service_frontend(FE));
        assert!(is_service_frontend(&format!("{FE}-IPv6")));
        for bad in [
            "",
            "kubernetes",
            "a0123",
            &format!("{FE}-"),
            // A full 32-digit UID is longer than the cloud provider's frontend names.
            &format!("{FE}f"),
            &FE.to_uppercase(),
            "b0123456789abcdef0123456789abcdef",
            "0b7a6f3e-3c1b-4d3e-9a55-5d2f7c3a1b2c",
        ] {
            assert!(!is_service_frontend(bad), "{bad}");
        }
        assert!(is_service_name("apps/web"));
        for bad in ["web", "apps/", "/web", "Apps/web", "apps/web/x"] {
            assert!(!is_service_name(bad), "{bad}");
        }
    }

    #[test]
    fn removes_the_frontend_first() {
        let lb = lb(&raw_lb(true));
        let owned = pip(Some(SERVICE), true);
        let p = plan(Some(&lb), Some(&owned));

        assert!(failed(&p.guards).is_empty());
        assert_eq!(p.step, Step::RemoveFrontend);
        assert!(p.remaining);
    }

    #[test]
    fn removed_frontend_takes_its_rules_and_unshared_probes() {
        let raw = raw_lb(true);
        let body = without_frontend(&raw, &lb(&raw), &lb_id(), FE);
        let names = |key: &str| -> Vec<String> {
            body["properties"][key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["name"].as_str().unwrap().to_owned())
                .collect()
        };

        assert_eq!(names("frontendIPConfigurations"), [OTHER]);
        assert_eq!(names("loadBalancingRules"), [format!("{OTHER}-TCP-80")]);
        assert_eq!(names("probes"), ["shared"]);
        assert_eq!(
            body["properties"]["backendAddressPools"],
            raw["properties"]["backendAddressPools"]
        );
        assert!(body.get("etag").is_none());
        assert_eq!(body["location"], "eastus");
    }

    #[test]
    fn deletes_the_load_balancer_with_its_last_frontend_if_it_has_no_members() {
        let mut raw = raw_lb(false);
        raw["properties"]["frontendIPConfigurations"]
            .as_array_mut()
            .unwrap()
            .truncate(1);

        assert_eq!(plan(Some(&lb(&raw)), None).step, Step::DeleteLoadBalancer);
        let mut with_members = raw_lb(true);
        with_members["properties"]["frontendIPConfigurations"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        let p = plan(Some(&lb(&with_members)), None);
        assert_eq!(p.step, Step::RemoveFrontend);
        assert_eq!(failed(&p.guards), ["not-last-frontend"]);
    }

    #[test]
    fn refuses_frontends_with_healthy_or_unknown_backends() {
        let lb = lb(&raw_lb(true));
        let owned = pip(Some(SERVICE), true);
        let with = |health: HashMap<String, Option<i64>>| {
            evaluate(
                Some(&lb),
                &lb_id(),
                Some(&owned),
                &pip_id(),
                FE,
                SERVICE,
                &health,
            )
            .guards
        };

        let mut healthy = down();
        healthy.insert(format!("{FE}-tcp-80"), Some(2));
        let healthy = with(healthy);
        assert_eq!(failed(&healthy), ["no-healthy-backends"]);
        assert!(healthy.iter().any(|g| g.detail.contains("-tcp-80 (2)")));

        let mut unknown = down();
        unknown.insert(format!("{FE}-tcp-443"), None);
        assert_eq!(failed(&with(unknown)), ["no-healthy-backends"]);
        assert!(failed(&with(down())).is_empty());
        assert!(
            failed(&with(HashMap::new())).is_empty(),
            "a frontend without rules"
        );
    }

    #[test]
    fn refuses_frontends_shared_with_other_services() {
        let mut raw = raw_lb(true);
        raw["properties"]["loadBalancingRules"]
            .as_array_mut()
            .unwrap()
            .push(rule(&format!("{OTHER}-TCP-8080"), FE, "shared"));
        let mut health = down();
        health.insert(format!("{OTHER}-tcp-8080"), Some(0));
        let guards = evaluate(
            Some(&lb(&raw)),
            &lb_id(),
            None,
            &pip_id(),
            FE,
            SERVICE,
            &health,
        )
        .guards;

        assert_eq!(failed(&guards), ["single-service"]);
    }

    #[test]
    fn checks_the_health_of_the_frontends_rules_only() {
        let (rules, _) = owned_by_frontend(&lb(&raw_lb(true)), &lb_id(), FE);
        let mut rules: Vec<String> = rules.into_iter().collect();
        rules.sort();

        let mut expected: Vec<String> = down().into_keys().collect();
        expected.sort();
        assert_eq!(rules, expected);
    }

    #[test]
    fn refuses_outbound_frontends_and_busy_load_balancers() {
        let mut raw = raw_lb(true);
        raw["properties"]["provisioningState"] = json!("Updating");
        raw["properties"]["frontendIPConfigurations"][0]["properties"]["outboundRules"] =
            json!([{"id": "/x/outboundRules/aksOutboundRule"}]);

        assert_eq!(
            failed(&plan(Some(&lb(&raw)), Some(&pip(Some(SERVICE), true))).guards),
            ["idle", "inbound-only"]
        );
    }

    #[test]
    fn deletes_the_public_ip_once_unused_and_only_if_owned() {
        let gone =
            lb(&json!({"name": "kubernetes", "properties": {"provisioningState": "Succeeded"}}));

        let p = plan(Some(&gone), Some(&pip(Some(SERVICE), false)));
        assert!(failed(&p.guards).is_empty());
        assert_eq!(p.step, Step::DeletePublicIp);
        assert!(!p.remaining);

        assert_eq!(
            failed(&plan(Some(&gone), Some(&pip(Some(SERVICE), true))).guards),
            ["public-ip-unused"]
        );
        assert_eq!(
            failed(&plan(None, Some(&pip(Some("apps/web, apps/api"), false))).guards),
            ["public-ip-owner"]
        );
        assert_eq!(
            failed(&plan(None, Some(&pip(Some("apps/other"), false))).guards),
            ["public-ip-owner"]
        );
        assert_eq!(
            failed(&plan(None, Some(&pip(None, false))).guards),
            ["public-ip-owner"]
        );
    }

    #[test]
    fn keeps_public_ips_aks_did_not_create_for_the_frontend() {
        let mut raw = raw_lb(true);
        raw["properties"]["frontendIPConfigurations"][0]["properties"]["publicIPAddress"] = json!({"id": "/subscriptions/s/resourceGroups/net/providers/Microsoft.Network/publicIPAddresses/static"});
        let p = plan(Some(&lb(&raw)), None);

        assert!(failed(&p.guards).is_empty());
        assert!(!p.remaining);
    }

    #[test]
    fn nothing_left_is_done() {
        let p = plan(None, None);

        assert!(failed(&p.guards).is_empty());
        assert_eq!(p.step, Step::Nothing);
    }
}
