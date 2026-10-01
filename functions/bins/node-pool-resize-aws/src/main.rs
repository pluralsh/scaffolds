//! Sets the desired size of an EKS managed node group or an Auto Scaling group.
//!
//! Exactly one target is required: either `clusterName` + `nodegroupName` (EKS managed node
//! group) or `autoScalingGroupName` (ASG directly). Groups scaled by the cluster autoscaler
//! are refused. The new count is capped by the installation's `MAX_NODE_COUNT`.

use aws_sdk_autoscaling::Client as AsgClient;
use aws_sdk_eks::Client as EksClient;
use aws_sdk_eks::error::ProvideErrorMetadata;
use aws_sdk_eks::types::{Nodegroup, NodegroupScalingConfig, NodegroupStatus};
use functions_aws::provider_error;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};

/// Nodes EKS allows in a managed node group (practical upper bound we also use for ASGs).
const AWS_MAX_COUNT: i64 = 1000;

/// Environment variable with the largest count callers may set, at most [`AWS_MAX_COUNT`].
const MAX_COUNT_VAR: &str = "MAX_NODE_COUNT";

/// Tag the cluster autoscaler sets when it owns an Auto Scaling group.
const CA_ENABLED_TAG: &str = "k8s.io/cluster-autoscaler/enabled";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    /// EKS cluster name (with `nodegroupName`).
    #[serde(default)]
    cluster_name: Option<String>,
    /// EKS managed node group name (with `clusterName`).
    #[serde(default)]
    nodegroup_name: Option<String>,
    /// Auto Scaling group name (standalone mode).
    #[serde(default)]
    auto_scaling_group_name: Option<String>,
    /// New desired number of nodes / instances.
    count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Eks,
    Asg,
}

#[derive(Debug, Clone)]
struct TargetView {
    kind: Target,
    name: String,
    /// Current desired size.
    desired: Option<i64>,
    min: Option<i64>,
    max: Option<i64>,
    state: String,
    /// Cluster autoscaler owns scaling.
    autoscaling: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetSummary {
    kind: &'static str,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    desired: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<i64>,
    state: String,
    autoscaling: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<TargetSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<i64>,
    to: i64,
    /// Whether the new desired size was submitted. Scaling continues in the background.
    submitted: bool,
}

fn validate(params: &Params) -> Result<Target, Error> {
    if params.count < 0 {
        return Err(Error::invalid_request("count can't be negative"));
    }
    let eks = params.cluster_name.as_ref().is_some_and(|s| !s.is_empty())
        || params
            .nodegroup_name
            .as_ref()
            .is_some_and(|s| !s.is_empty());
    let asg = params
        .auto_scaling_group_name
        .as_ref()
        .is_some_and(|s| !s.is_empty());

    match (eks, asg) {
        (true, false) => {
            let cluster = params.cluster_name.as_deref().unwrap_or("");
            let nodegroup = params.nodegroup_name.as_deref().unwrap_or("");
            if cluster.is_empty() || nodegroup.is_empty() {
                return Err(Error::invalid_request(
                    "clusterName and nodegroupName are both required for EKS managed node groups",
                ));
            }
            if !is_eks_name(cluster) {
                return Err(Error::invalid_request(format!(
                    "clusterName {cluster:?} is not a valid EKS cluster name"
                )));
            }
            if !is_eks_name(nodegroup) {
                return Err(Error::invalid_request(format!(
                    "nodegroupName {nodegroup:?} is not a valid EKS node group name"
                )));
            }
            Ok(Target::Eks)
        }
        (false, true) => {
            let name = params.auto_scaling_group_name.as_deref().unwrap();
            if name.len() > 255 || name.trim().is_empty() {
                return Err(Error::invalid_request(format!(
                    "autoScalingGroupName {name:?} is not a valid Auto Scaling group name"
                )));
            }
            Ok(Target::Asg)
        }
        (true, true) => Err(Error::invalid_request(
            "provide either clusterName+nodegroupName or autoScalingGroupName, not both",
        )),
        (false, false) => Err(Error::invalid_request(
            "provide clusterName+nodegroupName (EKS) or autoScalingGroupName (ASG)",
        )),
    }
}

fn is_eks_name(s: &str) -> bool {
    let len = s.len();
    (1..=100).contains(&len)
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

async fn handle(
    eks: &EksClient,
    asg: &AsgClient,
    req: Request<Params>,
) -> Result<Response<Output>, Error> {
    let params = req.params;
    let mode = validate(&params)?;
    let max = max_count();

    let target = match mode {
        Target::Eks => {
            find_nodegroup(
                eks,
                asg,
                params.cluster_name.as_deref().unwrap(),
                params.nodegroup_name.as_deref().unwrap(),
            )
            .await?
        }
        Target::Asg => find_asg(asg, params.auto_scaling_group_name.as_deref().unwrap()).await?,
    };

    let guards = evaluate(target.as_ref(), params.count, max);
    let mut output = Output {
        target: target.as_ref().map(summary),
        from: target.as_ref().and_then(|t| t.desired),
        to: params.count,
        submitted: false,
    };

    match (req.action, target) {
        (Action::Plan, _) => Ok(Response::planned(guards, output)),
        (Action::Execute, Some(target)) if guards.iter().all(|g| g.passed) => {
            if output.from == Some(params.count) {
                return Ok(Response::done(guards, output));
            }
            match target.kind {
                Target::Eks => {
                    scale_nodegroup(
                        eks,
                        params.cluster_name.as_deref().unwrap(),
                        params.nodegroup_name.as_deref().unwrap(),
                        &target,
                        params.count,
                    )
                    .await?;
                }
                Target::Asg => {
                    scale_asg(asg, &target.name, &target, params.count).await?;
                }
            }
            tracing::info!(
                kind = ?target.kind,
                name = %target.name,
                from = ?output.from,
                to = params.count,
                "scaling node group"
            );
            output.submitted = true;
            Ok(Response::done(guards, output))
        }
        (Action::Execute, _) => Ok(Response::refused(guards).with_result(output)),
    }
}

fn max_count() -> i64 {
    cap(std::env::var(MAX_COUNT_VAR).ok().as_deref())
}

fn cap(value: Option<&str>) -> i64 {
    value
        .and_then(|v| v.parse().ok())
        .map_or(AWS_MAX_COUNT, |max: i64| max.min(AWS_MAX_COUNT))
}

async fn find_nodegroup(
    eks: &EksClient,
    asg: &AsgClient,
    cluster: &str,
    nodegroup: &str,
) -> Result<Option<TargetView>, Error> {
    let out = match eks
        .describe_nodegroup()
        .cluster_name(cluster)
        .nodegroup_name(nodegroup)
        .send()
        .await
    {
        Ok(out) => out,
        Err(err) if err.code() == Some("ResourceNotFoundException") => return Ok(None),
        Err(err) => return Err(provider_error(err)),
    };
    let Some(ng) = out.nodegroup() else {
        return Ok(None);
    };
    let asg_names: Vec<String> = ng
        .resources()
        .map(|r| {
            r.auto_scaling_groups()
                .iter()
                .filter_map(|g| g.name().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let autoscaling = asg_has_cluster_autoscaler(asg, &asg_names).await?;
    Ok(Some(nodegroup_view(ng, autoscaling)))
}

fn nodegroup_view(ng: &Nodegroup, autoscaling: bool) -> TargetView {
    let scaling = ng.scaling_config();
    TargetView {
        kind: Target::Eks,
        name: ng.nodegroup_name().unwrap_or_default().to_owned(),
        desired: scaling.and_then(|s| s.desired_size().map(i64::from)),
        min: scaling.and_then(|s| s.min_size().map(i64::from)),
        max: scaling.and_then(|s| s.max_size().map(i64::from)),
        state: ng
            .status()
            .map(|s| s.as_str().to_owned())
            .unwrap_or_default(),
        autoscaling,
    }
}

async fn find_asg(asg: &AsgClient, name: &str) -> Result<Option<TargetView>, Error> {
    let out = asg
        .describe_auto_scaling_groups()
        .auto_scaling_group_names(name)
        .send()
        .await
        .map_err(provider_error)?;
    let Some(group) = out.auto_scaling_groups().first() else {
        return Ok(None);
    };
    let tags = group.tags();
    let autoscaling = tags.iter().any(|t| {
        t.key().is_some_and(|k| k == CA_ENABLED_TAG)
            && t.value().is_some_and(|v| v.eq_ignore_ascii_case("true"))
    });
    Ok(Some(TargetView {
        kind: Target::Asg,
        name: group.auto_scaling_group_name().unwrap_or(name).to_owned(),
        desired: group.desired_capacity().map(i64::from),
        min: group.min_size().map(i64::from),
        max: group.max_size().map(i64::from),
        state: "Active".into(),
        autoscaling,
    }))
}

async fn asg_has_cluster_autoscaler(asg: &AsgClient, names: &[String]) -> Result<bool, Error> {
    if names.is_empty() {
        return Ok(false);
    }
    let mut req = asg.describe_auto_scaling_groups();
    for name in names {
        req = req.auto_scaling_group_names(name);
    }
    let out = req.send().await.map_err(provider_error)?;
    Ok(out.auto_scaling_groups().iter().any(|g| {
        g.tags().iter().any(|t| {
            t.key().is_some_and(|k| k == CA_ENABLED_TAG)
                && t.value().is_some_and(|v| v.eq_ignore_ascii_case("true"))
        })
    }))
}

async fn scale_nodegroup(
    eks: &EksClient,
    cluster: &str,
    nodegroup: &str,
    target: &TargetView,
    count: i64,
) -> Result<(), Error> {
    // Keep min/max; only desired changes. Raise max if the new desired is above it.
    let min = target.min.unwrap_or(0).max(0) as i32;
    let max = target.max.unwrap_or(count).max(count) as i32;
    let desired = count as i32;
    eks.update_nodegroup_config()
        .cluster_name(cluster)
        .nodegroup_name(nodegroup)
        .scaling_config(
            NodegroupScalingConfig::builder()
                .min_size(min)
                .max_size(max)
                .desired_size(desired)
                .build(),
        )
        .send()
        .await
        .map_err(provider_error)?;
    Ok(())
}

async fn scale_asg(
    asg: &AsgClient,
    name: &str,
    target: &TargetView,
    count: i64,
) -> Result<(), Error> {
    let min = target.min.unwrap_or(0).min(count).max(0) as i32;
    let max = target.max.unwrap_or(count).max(count) as i32;
    asg.update_auto_scaling_group()
        .auto_scaling_group_name(name)
        .min_size(min)
        .max_size(max)
        .desired_capacity(count as i32)
        .send()
        .await
        .map_err(provider_error)?;
    Ok(())
}

fn evaluate(target: Option<&TargetView>, count: i64, max_allowed: i64) -> Vec<Guard> {
    let Some(target) = target else {
        return vec![Guard::fail("exists", "node group not found")];
    };
    let min_floor = 0;
    vec![
        Guard::pass(
            "exists",
            format!("{} {}", kind_label(target.kind), target.name),
        ),
        Guard::check(
            "idle",
            is_idle(target),
            format!(
                "state {}; another operation must finish first",
                target.state
            ),
        ),
        match target.autoscaling {
            true => Guard::fail(
                "manually-scaled",
                format!(
                    "the cluster autoscaler scales this group between {} and {}; change those instead",
                    target.min.map_or("?".into(), |c| c.to_string()),
                    target.max.map_or("?".into(), |c| c.to_string())
                ),
            ),
            false => Guard::pass(
                "manually-scaled",
                "cluster autoscaler not enabled on this group",
            ),
        },
        Guard::check(
            "count-allowed",
            (min_floor..=max_allowed).contains(&count),
            format!("{count} nodes; allowed {min_floor} to {max_allowed}"),
        ),
        // Also refuse counts above the group's current max when we would not raise max for ASG-only?
        // For EKS/ASG we raise max on execute, so only the installation cap applies.
    ]
}

fn is_idle(target: &TargetView) -> bool {
    match target.kind {
        Target::Eks => {
            target.state == NodegroupStatus::Active.as_str()
                || target.state.eq_ignore_ascii_case("ACTIVE")
        }
        Target::Asg => true,
    }
}

fn kind_label(kind: Target) -> &'static str {
    match kind {
        Target::Eks => "node group",
        Target::Asg => "Auto Scaling group",
    }
}

fn summary(target: &TargetView) -> TargetSummary {
    TargetSummary {
        kind: match target.kind {
            Target::Eks => "eks",
            Target::Asg => "asg",
        },
        name: target.name.clone(),
        desired: target.desired,
        min: target.min,
        max: target.max,
        state: target.state.clone(),
        autoscaling: target.autoscaling,
    }
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let cfg = functions_aws::sdk_config().await;
    let eks = EksClient::new(&cfg);
    let asg = AsgClient::new(&cfg);

    functions_aws::run(move |req| {
        let eks = eks.clone();
        let asg = asg.clone();
        async move { handle(&eks, &asg, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_eks(count: i64) -> Params {
        Params {
            cluster_name: Some("prod".into()),
            nodegroup_name: Some("apps".into()),
            auto_scaling_group_name: None,
            count,
        }
    }

    fn params_asg(count: i64) -> Params {
        Params {
            cluster_name: None,
            nodegroup_name: None,
            auto_scaling_group_name: Some("workers".into()),
            count,
        }
    }

    fn view(kind: Target, desired: i64, autoscaling: bool, state: &str) -> TargetView {
        TargetView {
            kind,
            name: "apps".into(),
            desired: Some(desired),
            min: Some(1),
            max: Some(5),
            state: state.into(),
            autoscaling,
        }
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    #[test]
    fn validates_modes() {
        assert_eq!(validate(&params_eks(2)).unwrap(), Target::Eks);
        assert_eq!(validate(&params_asg(2)).unwrap(), Target::Asg);
        assert!(
            validate(&Params {
                cluster_name: Some("prod".into()),
                nodegroup_name: Some("apps".into()),
                auto_scaling_group_name: Some("workers".into()),
                count: 1,
            })
            .is_err()
        );
        assert!(
            validate(&Params {
                cluster_name: None,
                nodegroup_name: None,
                auto_scaling_group_name: None,
                count: 1,
            })
            .is_err()
        );
        assert!(validate(&params_eks(-1)).is_err());
    }

    #[test]
    fn allows_idle_manual_groups() {
        let t = view(Target::Eks, 3, false, "ACTIVE");
        assert!(failed(&evaluate(Some(&t), 0, 10)).is_empty());
        assert!(failed(&evaluate(Some(&t), 10, 10)).is_empty());
        assert_eq!(failed(&evaluate(Some(&t), 11, 10)), ["count-allowed"]);
        assert_eq!(failed(&evaluate(None, 2, 10)), ["exists"]);
    }

    #[test]
    fn refuses_autoscaled_and_busy_eks() {
        let busy = view(Target::Eks, 3, true, "UPDATING");
        assert_eq!(
            failed(&evaluate(Some(&busy), 2, 10)),
            ["idle", "manually-scaled"]
        );
    }

    #[test]
    fn asg_is_always_idle_for_state() {
        let t = view(Target::Asg, 2, false, "Active");
        assert!(failed(&evaluate(Some(&t), 4, 10)).is_empty());
    }

    #[test]
    fn caps_max_count() {
        assert_eq!(cap(None), AWS_MAX_COUNT);
        assert_eq!(cap(Some("100")), 100);
        assert_eq!(cap(Some("99999")), AWS_MAX_COUNT);
    }
}
