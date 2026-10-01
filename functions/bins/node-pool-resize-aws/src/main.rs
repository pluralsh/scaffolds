//! Sets the desired size of an EKS managed node group or an Auto Scaling group.
//!
//! Exactly one target is required: either `clusterName` + `nodegroupName` (EKS managed node
//! group) or `autoScalingGroupName` (ASG directly). Groups scaled by the cluster autoscaler
//! are refused, and so are Auto Scaling groups that belong to an EKS managed node group: EKS
//! owns their size, so those are resized through the node group. The new count is capped by
//! the installation's `MAX_NODE_COUNT`.
//!
//! The group's own minimum and maximum must contain the new desired size, so `execute` lowers
//! the minimum or raises the maximum when it has to. `plan` reports the limits it would set.

use aws_sdk_autoscaling::Client as AsgClient;
use aws_sdk_eks::Client as EksClient;
use aws_sdk_eks::error::ProvideErrorMetadata;
use aws_sdk_eks::types::{Nodegroup, NodegroupScalingConfig, NodegroupStatus};
use functions_aws::provider_error;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};

/// Largest size EKS allows for a managed node group, also used as the limit for ASGs.
const AWS_MAX_COUNT: i64 = 1000;

/// Environment variable with the largest count callers may set, at most [`AWS_MAX_COUNT`].
const MAX_COUNT_VAR: &str = "MAX_NODE_COUNT";

/// Tag the cluster autoscaler sets when it owns an Auto Scaling group.
const CA_ENABLED_TAG: &str = "k8s.io/cluster-autoscaler/enabled";

/// Tag EKS sets on the Auto Scaling group of a managed node group.
const EKS_NODEGROUP_TAG: &str = "eks:nodegroup-name";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    /// EKS cluster name (with `nodegroupName`).
    #[serde(default)]
    cluster_name: Option<String>,
    /// EKS managed node group name (with `clusterName`).
    #[serde(default)]
    nodegroup_name: Option<String>,
    /// Auto Scaling group name, used instead of `clusterName` and `nodegroupName`.
    #[serde(default)]
    auto_scaling_group_name: Option<String>,
    /// New desired number of nodes.
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
    /// Whether the cluster autoscaler scales the group.
    autoscaling: bool,
    /// EKS managed node group an Auto Scaling group belongs to, which owns its size.
    eks_nodegroup: Option<String>,
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
    /// The group's minimum and maximum size after the change, see [`limits`].
    #[serde(skip_serializing_if = "Option::is_none")]
    new_min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_max: Option<i64>,
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
    let new_limits = target.as_ref().map(|t| limits(t, params.count));
    let mut output = Output {
        target: target.as_ref().map(summary),
        from: target.as_ref().and_then(|t| t.desired),
        to: params.count,
        new_min: new_limits.map(|(min, _)| min),
        new_max: new_limits.map(|(_, max)| max),
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
        eks_nodegroup: None,
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
    let eks_nodegroup = tags
        .iter()
        .find(|t| t.key() == Some(EKS_NODEGROUP_TAG))
        .and_then(|t| t.value())
        .map(str::to_owned);
    Ok(Some(TargetView {
        kind: Target::Asg,
        name: group.auto_scaling_group_name().unwrap_or(name).to_owned(),
        desired: group.desired_capacity().map(i64::from),
        min: group.min_size().map(i64::from),
        max: group.max_size().map(i64::from),
        state: "Active".into(),
        autoscaling,
        eks_nodegroup,
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
    let (min, max) = limits(target, count);
    let (min, max, desired) = (min as i32, max as i32, count as i32);
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
    let (min, max) = limits(target, count);
    let (min, max) = (min as i32, max as i32);
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

/// The minimum and maximum size to set with the new desired size: the group's own, widened to
/// contain it. EKS and Auto Scaling both reject a desired size outside them, and EKS needs a
/// maximum of at least 1.
fn limits(target: &TargetView, count: i64) -> (i64, i64) {
    let min = target.min.unwrap_or(count).min(count).max(0);
    let max = target.max.unwrap_or(count).max(count);
    match target.kind {
        Target::Eks => (min, max.max(1)),
        Target::Asg => (min, max),
    }
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
        match &target.eks_nodegroup {
            Some(nodegroup) => Guard::fail(
                "not-eks-managed",
                format!(
                    "belongs to EKS node group {nodegroup}; set clusterName and nodegroupName instead so EKS stays in charge"
                ),
            ),
            None => Guard::pass("not-eks-managed", "not part of an EKS managed node group"),
        },
        Guard::check(
            "count-allowed",
            (min_floor..=max_allowed).contains(&count),
            format!("{count} nodes; allowed {min_floor} to {max_allowed}"),
        ),
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
            eks_nodegroup: None,
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
    fn refuses_asgs_of_eks_node_groups() {
        let mut t = view(Target::Asg, 2, false, "Active");
        t.eks_nodegroup = Some("apps".into());

        assert_eq!(failed(&evaluate(Some(&t), 4, 10)), ["not-eks-managed"]);
    }

    #[test]
    fn widens_the_limits_to_contain_the_count() {
        let mut t = view(Target::Asg, 3, false, "Active");
        t.min = Some(2);
        t.max = Some(4);

        assert_eq!(limits(&t, 3), (2, 4));
        assert_eq!(limits(&t, 0), (0, 4), "min is lowered");
        assert_eq!(limits(&t, 6), (2, 6), "max is raised");
        t.max = Some(0);
        t.min = Some(0);
        assert_eq!(limits(&t, 0), (0, 0));
        t.kind = Target::Eks;
        assert_eq!(limits(&t, 0), (0, 1), "EKS needs a maximum of at least 1");
    }

    #[test]
    fn caps_max_count() {
        assert_eq!(cap(None), AWS_MAX_COUNT);
        assert_eq!(cap(Some("100")), 100);
        assert_eq!(cap(Some("99999")), AWS_MAX_COUNT);
    }
}

/// The handler against scripted EKS and Auto Scaling responses, asserting on the changes it
/// sends.
#[cfg(test)]
mod handler_tests {
    use aws_sdk_autoscaling::operation::describe_auto_scaling_groups::DescribeAutoScalingGroupsOutput;
    use aws_sdk_autoscaling::operation::update_auto_scaling_group::UpdateAutoScalingGroupOutput;
    use aws_sdk_autoscaling::types::{AutoScalingGroup, TagDescription};
    use aws_sdk_eks::error::ErrorMetadata;
    use aws_sdk_eks::operation::describe_nodegroup::{
        DescribeNodegroupError, DescribeNodegroupOutput,
    };
    use aws_sdk_eks::operation::update_nodegroup_config::UpdateNodegroupConfigOutput;
    use aws_sdk_eks::types::{NodegroupResources, NodegroupStatus};
    use aws_smithy_mocks::{Rule, RuleMode, mock, mock_client};
    use serde_json::{Value, json};

    use super::*;

    /// The limits and desired size a change is expected to set.
    type Expect = (i32, i32, i32);

    fn tag(key: &str, value: &str) -> TagDescription {
        TagDescription::builder().key(key).value(value).build()
    }

    fn group(min: i32, max: i32, desired: i32, tags: &[(&str, &str)]) -> AutoScalingGroup {
        let mut builder = AutoScalingGroup::builder()
            .auto_scaling_group_name("workers")
            .min_size(min)
            .max_size(max)
            .desired_capacity(desired);
        for (key, value) in tags {
            builder = builder.tags(tag(key, value));
        }
        builder.build()
    }

    fn nodegroup(status: NodegroupStatus, min: i32, max: i32, desired: i32) -> Nodegroup {
        Nodegroup::builder()
            .nodegroup_name("apps")
            .status(status)
            .scaling_config(
                NodegroupScalingConfig::builder()
                    .min_size(min)
                    .max_size(max)
                    .desired_size(desired)
                    .build(),
            )
            .resources(
                NodegroupResources::builder()
                    .auto_scaling_groups(
                        aws_sdk_eks::types::AutoScalingGroup::builder()
                            .name("workers")
                            .build(),
                    )
                    .build(),
            )
            .build()
    }

    struct Aws {
        eks: EksClient,
        asg: AsgClient,
        update_nodegroup: Rule,
        update_asg: Rule,
    }

    impl Aws {
        /// `nodegroup` is the EKS node group to find, `groups` the Auto Scaling groups
        /// describing returns, and `expect` the limits and desired size the update must set.
        fn new(
            nodegroup: Option<Nodegroup>,
            groups: Vec<AutoScalingGroup>,
            expect: Expect,
        ) -> Self {
            let describe_nodegroup = match nodegroup {
                Some(ng) => mock!(EksClient::describe_nodegroup).then_output(move || {
                    DescribeNodegroupOutput::builder()
                        .nodegroup(ng.clone())
                        .build()
                }),
                None => mock!(EksClient::describe_nodegroup).then_error(|| {
                    DescribeNodegroupError::generic(
                        ErrorMetadata::builder()
                            .code("ResourceNotFoundException")
                            .build(),
                    )
                }),
            };
            let (min, max, desired) = expect;
            // A request with other limits matches no rule, which fails the test.
            let update_nodegroup = mock!(EksClient::update_nodegroup_config)
                .match_requests(move |req| {
                    let scaling = req.scaling_config();
                    req.cluster_name() == Some("prod")
                        && req.nodegroup_name() == Some("apps")
                        && scaling.and_then(|s| s.min_size()) == Some(min)
                        && scaling.and_then(|s| s.max_size()) == Some(max)
                        && scaling.and_then(|s| s.desired_size()) == Some(desired)
                })
                .then_output(|| UpdateNodegroupConfigOutput::builder().build());
            let eks = mock_client!(
                aws_sdk_eks,
                RuleMode::MatchAny,
                [&describe_nodegroup, &update_nodegroup]
            );

            let describe_groups =
                mock!(AsgClient::describe_auto_scaling_groups).then_output(move || {
                    DescribeAutoScalingGroupsOutput::builder()
                        .set_auto_scaling_groups(Some(groups.clone()))
                        .build()
                });
            let update_asg = mock!(AsgClient::update_auto_scaling_group)
                .match_requests(move |req| {
                    req.auto_scaling_group_name() == Some("workers")
                        && req.min_size() == Some(min)
                        && req.max_size() == Some(max)
                        && req.desired_capacity() == Some(desired)
                })
                .then_output(|| UpdateAutoScalingGroupOutput::builder().build());
            let asg = mock_client!(
                aws_sdk_autoscaling,
                RuleMode::MatchAny,
                [&describe_groups, &update_asg]
            );

            Self {
                eks,
                asg,
                update_nodegroup,
                update_asg,
            }
        }

        async fn call(&self, params: Value) -> Value {
            let req = serde_json::from_value(params).unwrap();
            serde_json::to_value(handle(&self.eks, &self.asg, req).await.unwrap()).unwrap()
        }

        /// How many times the function resized the node group and the Auto Scaling group.
        fn writes(&self) -> (usize, usize) {
            (
                self.update_nodegroup.num_calls(),
                self.update_asg.num_calls(),
            )
        }
    }

    fn asg_request(action: &str, count: i64) -> Value {
        json!({"action": action, "autoScalingGroupName": "workers", "count": count})
    }

    fn eks_request(action: &str, count: i64) -> Value {
        json!({"action": action, "clusterName": "prod", "nodegroupName": "apps", "count": count})
    }

    fn failed(resp: &Value) -> Vec<&str> {
        resp["guards"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| g["passed"] == false)
            .map(|g| g["name"].as_str().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn plan_reports_the_limits_without_changing_anything() {
        let aws = Aws::new(None, vec![group(2, 4, 3, &[])], (0, 4, 0));

        let resp = aws.call(asg_request("plan", 0)).await;

        assert_eq!(resp["outcome"], "planned");
        assert_eq!(resp["result"]["from"], 3);
        assert_eq!(resp["result"]["newMin"], 0);
        assert_eq!(resp["result"]["newMax"], 4);
        assert_eq!(resp["result"]["submitted"], false);
        assert_eq!(aws.writes(), (0, 0));
    }

    #[tokio::test]
    async fn scales_an_asg_lowering_its_minimum() {
        let aws = Aws::new(None, vec![group(2, 4, 3, &[])], (0, 4, 0));

        let resp = aws.call(asg_request("execute", 0)).await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["submitted"], true);
        assert_eq!(aws.writes(), (0, 1));
    }

    #[tokio::test]
    async fn scales_an_asg_raising_its_maximum() {
        let aws = Aws::new(None, vec![group(2, 4, 3, &[])], (2, 6, 6));

        let resp = aws.call(asg_request("execute", 6)).await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(aws.writes(), (0, 1));
    }

    #[tokio::test]
    async fn does_nothing_when_the_size_is_already_right() {
        let aws = Aws::new(None, vec![group(2, 4, 3, &[])], (2, 4, 3));

        let resp = aws.call(asg_request("execute", 3)).await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["submitted"], false);
        assert_eq!(aws.writes(), (0, 0));
    }

    #[tokio::test]
    async fn never_resizes_autoscaler_or_eks_owned_asgs() {
        for tags in [
            vec![(CA_ENABLED_TAG, "true")],
            vec![(EKS_NODEGROUP_TAG, "apps")],
        ] {
            let aws = Aws::new(None, vec![group(2, 4, 3, &tags)], (0, 4, 0));

            let resp = aws.call(asg_request("execute", 0)).await;

            assert_eq!(resp["outcome"], "refused", "{tags:?}");
            assert_eq!(aws.writes(), (0, 0), "{tags:?}");
        }
    }

    #[tokio::test]
    async fn reports_a_missing_asg() {
        let aws = Aws::new(None, vec![], (0, 0, 0));

        let resp = aws.call(asg_request("execute", 1)).await;

        assert_eq!(resp["outcome"], "refused");
        assert_eq!(failed(&resp), ["exists"]);
    }

    #[tokio::test]
    async fn scales_a_node_group_lowering_its_minimum() {
        let ng = nodegroup(NodegroupStatus::Active, 1, 5, 3);
        let aws = Aws::new(Some(ng), vec![group(1, 5, 3, &[])], (0, 5, 0));

        let resp = aws.call(eks_request("execute", 0)).await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["target"]["kind"], "eks");
        assert_eq!(resp["result"]["submitted"], true);
        assert_eq!(aws.writes(), (1, 0));
    }

    #[tokio::test]
    async fn scales_a_node_group_raising_its_maximum() {
        let ng = nodegroup(NodegroupStatus::Active, 1, 5, 3);
        let aws = Aws::new(Some(ng), vec![group(1, 5, 3, &[])], (1, 8, 8));

        let resp = aws.call(eks_request("execute", 8)).await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(aws.writes(), (1, 0));
    }

    #[tokio::test]
    async fn never_resizes_busy_or_autoscaled_node_groups() {
        let busy = nodegroup(NodegroupStatus::Updating, 1, 5, 3);
        let aws = Aws::new(Some(busy), vec![group(1, 5, 3, &[])], (0, 5, 0));
        let resp = aws.call(eks_request("execute", 0)).await;
        assert_eq!(failed(&resp), ["idle"]);
        assert_eq!(aws.writes(), (0, 0));

        // The autoscaler tags the Auto Scaling group behind the node group.
        let active = nodegroup(NodegroupStatus::Active, 1, 5, 3);
        let aws = Aws::new(
            Some(active),
            vec![group(1, 5, 3, &[(CA_ENABLED_TAG, "true")])],
            (0, 5, 0),
        );
        let resp = aws.call(eks_request("execute", 0)).await;
        assert_eq!(failed(&resp), ["manually-scaled"]);
        assert_eq!(aws.writes(), (0, 0));
    }

    #[tokio::test]
    async fn reports_a_missing_node_group() {
        let aws = Aws::new(None, vec![], (0, 0, 0));

        let resp = aws.call(eks_request("execute", 1)).await;

        assert_eq!(resp["outcome"], "refused");
        assert_eq!(failed(&resp), ["exists"]);
    }
}
