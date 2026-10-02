//! Deletes an orphaned Kubernetes load balancer, and the target groups it leaves behind, on AWS.
//!
//! The AWS Load Balancer Controller creates an Application or Network Load Balancer, with its
//! target groups, for every Service or Ingress, and the Kubernetes cloud provider does the same
//! for Network Load Balancers. When the Service or Ingress is deleted without them being cleaned
//! up, they stay and cost money. The caller names the load balancer, the cluster and the Service
//! or Ingress (`namespace/name`) it was created for and confirms in the cluster that it no
//! longer exists. The function checks that the load balancer is tagged as created for exactly
//! that, that it isn't protected against deletion and that none of its target groups has
//! healthy targets, which would mean traffic still flows through it.
//!
//! Each `execute` makes one change, because a target group can only be deleted once the load
//! balancer using it is gone:
//!
//! 1. deletes the load balancer, which deletes its listeners;
//! 2. a later `execute` finds the target groups left for that Service or Ingress by their tags
//!    and deletes those that no load balancer uses and that have no healthy targets.
//!
//! Classic Load Balancers have no target groups and another API, and aren't supported.

use std::collections::HashMap;

use aws_sdk_elasticloadbalancingv2::Client;
use aws_sdk_elasticloadbalancingv2::error::ProvideErrorMetadata;
use aws_sdk_elasticloadbalancingv2::types::{
    LoadBalancer as SdkLoadBalancer, LoadBalancerStateEnum, TargetGroup as SdkTargetGroup,
    TargetHealthStateEnum,
};
use functions_aws::provider_error;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};

/// Tags the AWS Load Balancer Controller sets on what it creates: the cluster, and the stack
/// the resource belongs to, `namespace/name` of the Service or Ingress (or the name of an
/// Ingress group).
const CONTROLLER_CLUSTER_TAG: &str = "elbv2.k8s.aws/cluster";
const SERVICE_STACK_TAG: &str = "service.k8s.aws/stack";
const INGRESS_STACK_TAG: &str = "ingress.k8s.aws/stack";

/// Tags the Kubernetes cloud provider sets on the load balancers of Services: the Service as
/// `namespace/name` and `owned` for the cluster.
const SERVICE_NAME_TAG: &str = "kubernetes.io/service-name";
const CLUSTER_TAG_PREFIX: &str = "kubernetes.io/cluster/";

/// Attribute that protects a load balancer from deletion.
const DELETION_PROTECTION_ATTRIBUTE: &str = "deletion_protection.enabled";

/// `DescribeTags` accepts at most this many resources.
const TAGS_BATCH: usize = 20;

type Tags = HashMap<String, String>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    load_balancer_arn: String,
    /// EKS cluster the load balancer was created for.
    cluster_name: String,
    /// `namespace/name` of the Service or Ingress, or the name of the Ingress group; the
    /// caller confirms it no longer exists.
    service_name: String,
}

#[derive(Debug, Clone)]
struct LoadBalancer {
    arn: String,
    name: String,
    kind: String,
    scheme: Option<String>,
    state: String,
    deletion_protection: bool,
    tags: Tags,
}

#[derive(Debug, Clone)]
struct TargetGroup {
    arn: String,
    name: String,
    port: Option<i32>,
    protocol: Option<String>,
    /// Load balancers whose listeners use the target group.
    load_balancers: Vec<String>,
    /// Targets that are healthy or still registering.
    live_targets: usize,
    targets: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadBalancerSummary {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheme: Option<String>,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetGroupSummary {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
    /// Whether a load balancer still uses it.
    attached: bool,
    /// Healthy targets and targets that are still registering.
    live_targets: usize,
    targets: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    load_balancer: Option<LoadBalancerSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    target_groups: Vec<TargetGroupSummary>,
    load_balancer_deleted: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    deleted_target_groups: Vec<String>,
    /// Whether another `execute` is needed to delete the target groups left over.
    remaining: bool,
}

/// What the next `execute` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    DeleteLoadBalancer,
    DeleteTargetGroups,
    Nothing,
}

fn validate(params: &Params) -> Result<(), Error> {
    if !is_load_balancer_arn(&params.load_balancer_arn) {
        return Err(Error::invalid_request(format!(
            "loadBalancerArn {:?} is not an Application or Network Load Balancer ARN (arn:aws:elasticloadbalancing:<region>:<account>:loadbalancer/app|net/<name>/<id>)",
            params.load_balancer_arn
        )));
    }
    if !is_cluster_name(&params.cluster_name) {
        return Err(Error::invalid_request(format!(
            "clusterName {:?} is not an EKS cluster name",
            params.cluster_name
        )));
    }
    if !is_stack_name(&params.service_name) {
        return Err(Error::invalid_request(format!(
            "serviceName {:?} is not a Service or Ingress as namespace/name",
            params.service_name
        )));
    }
    Ok(())
}

/// `arn:<partition>:elasticloadbalancing:<region>:<account>:loadbalancer/<app|net>/<name>/<id>`
fn is_load_balancer_arn(arn: &str) -> bool {
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    let [prefix, partition, service, region, account, resource] = parts.as_slice() else {
        return false;
    };
    if *prefix != "arn"
        || !partition.starts_with("aws")
        || *service != "elasticloadbalancing"
        || region.is_empty()
        || account.len() != 12
        || !account.bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let path: Vec<&str> = resource.split('/').collect();
    let [kind, scheme, name, id] = path.as_slice() else {
        return false;
    };
    *kind == "loadbalancer"
        && matches!(*scheme, "app" | "net")
        && (1..=32).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && id.len() == 16
        && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_cluster_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `namespace/name`, or the bare name of an Ingress group.
fn is_stack_name(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
    };
    name.len() <= 253 && name.split('/').count() <= 2 && name.split('/').all(part)
}

async fn handle(elb: &Client, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    validate(&params)?;

    let load_balancer = find_load_balancer(elb, &params.load_balancer_arn).await?;
    let target_groups = match &load_balancer {
        Some(lb) => target_groups_of(elb, &lb.arn).await?,
        None => leftover_target_groups(elb, &params.cluster_name, &params.service_name).await?,
    };

    let guards = evaluate(
        load_balancer.as_ref(),
        &target_groups,
        &params.cluster_name,
        &params.service_name,
    );
    let step = match (&load_balancer, target_groups.is_empty()) {
        (Some(_), _) => Step::DeleteLoadBalancer,
        (None, false) => Step::DeleteTargetGroups,
        (None, true) => Step::Nothing,
    };
    let mut output = Output {
        load_balancer: load_balancer.as_ref().map(load_balancer_summary),
        target_groups: target_groups.iter().map(target_group_summary).collect(),
        load_balancer_deleted: false,
        deleted_target_groups: Vec::new(),
        remaining: step == Step::DeleteLoadBalancer && !target_groups.is_empty(),
    };

    if req.action == Action::Plan {
        return Ok(Response::planned(guards, output));
    }
    if !guards.iter().all(|g| g.passed) {
        return Ok(Response::refused(guards).with_result(output));
    }

    match step {
        Step::DeleteLoadBalancer => {
            elb.delete_load_balancer()
                .load_balancer_arn(&params.load_balancer_arn)
                .send()
                .await
                .map_err(provider_error)?;
            tracing::info!(arn = %params.load_balancer_arn, "deleted load balancer");
            output.load_balancer_deleted = true;
        }
        Step::DeleteTargetGroups => {
            for group in &target_groups {
                elb.delete_target_group()
                    .target_group_arn(&group.arn)
                    .send()
                    .await
                    .map_err(provider_error)?;
                tracing::info!(arn = %group.arn, "deleted target group");
                output.deleted_target_groups.push(group.name.clone());
            }
            output.remaining = false;
        }
        // The guards already refused: there is nothing to delete.
        Step::Nothing => return Ok(Response::refused(guards).with_result(output)),
    }
    Ok(Response::done(guards, output))
}

async fn find_load_balancer(elb: &Client, arn: &str) -> Result<Option<LoadBalancer>, Error> {
    let out = match elb
        .describe_load_balancers()
        .load_balancer_arns(arn)
        .send()
        .await
    {
        Ok(out) => out,
        Err(err) if err.code() == Some("LoadBalancerNotFound") => return Ok(None),
        Err(err) => return Err(provider_error(err)),
    };
    let Some(lb) = out.load_balancers().first() else {
        return Ok(None);
    };
    let tags = tags_of(elb, &[arn.to_owned()])
        .await?
        .remove(arn)
        .unwrap_or_default();
    let attributes = elb
        .describe_load_balancer_attributes()
        .load_balancer_arn(arn)
        .send()
        .await
        .map_err(provider_error)?;
    let protected = attributes
        .attributes()
        .iter()
        .any(|a| a.key() == Some(DELETION_PROTECTION_ATTRIBUTE) && a.value() == Some("true"));
    Ok(Some(load_balancer_from(lb, protected, tags)))
}

fn load_balancer_from(lb: &SdkLoadBalancer, protected: bool, tags: Tags) -> LoadBalancer {
    LoadBalancer {
        arn: lb.load_balancer_arn().unwrap_or_default().to_owned(),
        name: lb.load_balancer_name().unwrap_or_default().to_owned(),
        kind: lb
            .r#type()
            .map(|t| t.as_str().to_owned())
            .unwrap_or_default(),
        scheme: lb.scheme().map(|s| s.as_str().to_owned()),
        state: lb
            .state()
            .and_then(|s| s.code())
            .map(|c| c.as_str().to_owned())
            .unwrap_or_default(),
        deletion_protection: protected,
        tags,
    }
}

/// Tags of the resources, by ARN.
async fn tags_of(elb: &Client, arns: &[String]) -> Result<HashMap<String, Tags>, Error> {
    let mut all = HashMap::new();
    for batch in arns.chunks(TAGS_BATCH) {
        let out = elb
            .describe_tags()
            .set_resource_arns(Some(batch.to_vec()))
            .send()
            .await
            .map_err(provider_error)?;
        for description in out.tag_descriptions() {
            let tags = description
                .tags()
                .iter()
                .filter_map(|t| {
                    Some((
                        t.key()?.to_owned(),
                        t.value().unwrap_or_default().to_owned(),
                    ))
                })
                .collect();
            if let Some(arn) = description.resource_arn() {
                all.insert(arn.to_owned(), tags);
            }
        }
    }
    Ok(all)
}

/// Every target group of the region, or the ones the load balancer uses.
async fn describe_target_groups(
    elb: &Client,
    load_balancer_arn: Option<&str>,
) -> Result<Vec<SdkTargetGroup>, Error> {
    let mut groups = Vec::new();
    let mut marker = None;
    loop {
        let out = elb
            .describe_target_groups()
            .set_load_balancer_arn(load_balancer_arn.map(str::to_owned))
            .set_marker(marker)
            .send()
            .await
            .map_err(provider_error)?;
        groups.extend(out.target_groups().iter().cloned());
        marker = out.next_marker().map(str::to_owned);
        if marker.is_none() {
            return Ok(groups);
        }
    }
}

/// The target groups the load balancer uses.
async fn target_groups_of(
    elb: &Client,
    load_balancer_arn: &str,
) -> Result<Vec<TargetGroup>, Error> {
    let groups = describe_target_groups(elb, Some(load_balancer_arn)).await?;
    with_health(elb, &groups).await
}

/// The target groups created for the Service or Ingress, found by their tags. Groups a load
/// balancer still uses are included, so the guards can refuse them.
async fn leftover_target_groups(
    elb: &Client,
    cluster: &str,
    service: &str,
) -> Result<Vec<TargetGroup>, Error> {
    let groups = describe_target_groups(elb, None).await?;
    let arns: Vec<String> = groups
        .iter()
        .filter_map(|g| g.target_group_arn().map(str::to_owned))
        .collect();
    let tags = tags_of(elb, &arns).await?;
    let owned: Vec<SdkTargetGroup> = groups
        .into_iter()
        .filter(|g| {
            g.target_group_arn()
                .and_then(|arn| tags.get(arn))
                .is_some_and(|tags| ownership(tags, cluster, service).is_ok())
        })
        .collect();
    with_health(elb, &owned).await
}

async fn with_health(elb: &Client, groups: &[SdkTargetGroup]) -> Result<Vec<TargetGroup>, Error> {
    let mut result = Vec::new();
    for group in groups {
        let arn = group.target_group_arn().unwrap_or_default();
        let health = elb
            .describe_target_health()
            .target_group_arn(arn)
            .send()
            .await
            .map_err(provider_error)?;
        let states: Vec<&TargetHealthStateEnum> = health
            .target_health_descriptions()
            .iter()
            .filter_map(|d| d.target_health().and_then(|h| h.state()))
            .collect();
        result.push(TargetGroup {
            arn: arn.to_owned(),
            name: group.target_group_name().unwrap_or_default().to_owned(),
            port: group.port(),
            protocol: group.protocol().map(|p| p.as_str().to_owned()),
            load_balancers: group.load_balancer_arns().to_vec(),
            live_targets: states.iter().filter(|s| is_live(s)).count(),
            targets: health.target_health_descriptions().len(),
        });
    }
    Ok(result)
}

/// Whether a target receives traffic or is about to.
fn is_live(state: &TargetHealthStateEnum) -> bool {
    matches!(
        state,
        TargetHealthStateEnum::Healthy | TargetHealthStateEnum::Initial
    )
}

/// Whether the tags say the resource was created for the Service or Ingress in the cluster,
/// and by which controller.
fn ownership(tags: &Tags, cluster: &str, service: &str) -> Result<&'static str, String> {
    let tag = |key: &str| tags.get(key).map(String::as_str);
    let stack = [SERVICE_STACK_TAG, INGRESS_STACK_TAG]
        .into_iter()
        .any(|key| tag(key) == Some(service));
    if tag(CONTROLLER_CLUSTER_TAG) == Some(cluster) && stack {
        return Ok("AWS Load Balancer Controller");
    }
    let owned = tag(&format!("{CLUSTER_TAG_PREFIX}{cluster}")) == Some("owned");
    if owned && tag(SERVICE_NAME_TAG) == Some(service) {
        return Ok("Kubernetes cloud provider");
    }
    Err(format!(
        "not tagged as created for {service} in cluster {cluster}: expected {CONTROLLER_CLUSTER_TAG}={cluster} with {SERVICE_STACK_TAG} or {INGRESS_STACK_TAG}={service}, or {SERVICE_NAME_TAG}={service} with {CLUSTER_TAG_PREFIX}{cluster}=owned"
    ))
}

fn evaluate(
    load_balancer: Option<&LoadBalancer>,
    target_groups: &[TargetGroup],
    cluster: &str,
    service: &str,
) -> Vec<Guard> {
    let healthy = Guard::check(
        "no-healthy-targets",
        target_groups.iter().all(|g| g.live_targets == 0),
        live_targets_detail(target_groups),
    );
    let Some(lb) = load_balancer else {
        if target_groups.is_empty() {
            return vec![Guard::fail(
                "exists",
                format!("load balancer not found, and no target groups are left for {service}"),
            )];
        }
        return vec![
            Guard::pass(
                "exists",
                format!(
                    "load balancer already deleted; {} target group(s) left for {service}",
                    target_groups.len()
                ),
            ),
            Guard::check(
                "unattached",
                target_groups.iter().all(|g| g.load_balancers.is_empty()),
                attached_detail(target_groups),
            ),
            healthy,
        ];
    };

    vec![
        Guard::pass("exists", format!("load balancer {}", lb.name)),
        match ownership(&lb.tags, cluster, service) {
            Ok(by) => Guard::pass(
                "kubernetes",
                format!("created by the {by} for {service} in cluster {cluster}"),
            ),
            Err(detail) => Guard::fail("kubernetes", detail),
        },
        Guard::check(
            "deletion-protection",
            !lb.deletion_protection,
            if lb.deletion_protection {
                "deletion protection is enabled; disable it first"
            } else {
                "deletion protection is off"
            },
        ),
        Guard::check(
            "state",
            lb.state != LoadBalancerStateEnum::Provisioning.as_str(),
            format!("state {}", lb.state),
        ),
        healthy,
    ]
}

fn live_targets_detail(target_groups: &[TargetGroup]) -> String {
    let live: Vec<String> = target_groups
        .iter()
        .filter(|g| g.live_targets > 0)
        .map(|g| format!("{} ({} of {} targets)", g.name, g.live_targets, g.targets))
        .collect();
    if live.is_empty() {
        "no healthy or registering targets".into()
    } else {
        format!(
            "target groups with healthy or registering targets, which still receive traffic: {}",
            live.join(", ")
        )
    }
}

fn attached_detail(target_groups: &[TargetGroup]) -> String {
    let attached: Vec<&str> = target_groups
        .iter()
        .filter(|g| !g.load_balancers.is_empty())
        .map(|g| g.name.as_str())
        .collect();
    if attached.is_empty() {
        "no load balancer uses them".into()
    } else {
        format!("still used by a load balancer: {}", attached.join(", "))
    }
}

fn load_balancer_summary(lb: &LoadBalancer) -> LoadBalancerSummary {
    LoadBalancerSummary {
        name: lb.name.clone(),
        kind: lb.kind.clone(),
        scheme: lb.scheme.clone(),
        state: lb.state.clone(),
    }
}

fn target_group_summary(group: &TargetGroup) -> TargetGroupSummary {
    TargetGroupSummary {
        name: group.name.clone(),
        port: group.port,
        protocol: group.protocol.clone(),
        attached: !group.load_balancers.is_empty(),
        live_targets: group.live_targets,
        targets: group.targets,
    }
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let elb = Client::new(&functions_aws::sdk_config().await);

    functions_aws::run(move |req| {
        let elb = elb.clone();
        async move { handle(&elb, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARN: &str = "arn:aws:elasticloadbalancing:us-east-2:123456789012:loadbalancer/app/k8s-apps-web/0123456789abcdef";

    fn params(arn: &str, cluster: &str, service: &str) -> Params {
        Params {
            load_balancer_arn: arn.into(),
            cluster_name: cluster.into(),
            service_name: service.into(),
        }
    }

    fn tags(pairs: &[(&str, &str)]) -> Tags {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn validates_the_inputs() {
        assert!(validate(&params(ARN, "prod", "apps/web")).is_ok());
        assert!(validate(&params(ARN, "prod", "ingress-group")).is_ok());
        assert!(
            validate(&params(
                &ARN.replace("loadbalancer/app", "loadbalancer/net"),
                "prod",
                "apps/web"
            ))
            .is_ok()
        );
        for arn in [
            "",
            "lb-1",
            // Gateway load balancers have no Kubernetes controller.
            "arn:aws:elasticloadbalancing:us-east-2:123456789012:loadbalancer/gwy/x/0123456789abcdef",
            // A target group, not a load balancer.
            "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/web/0123456789abcdef",
            "arn:aws:elasticloadbalancing:us-east-2:12345:loadbalancer/app/web/0123456789abcdef",
            "arn:aws:elasticloadbalancing:us-east-2:123456789012:loadbalancer/app/web/xyz",
        ] {
            assert!(validate(&params(arn, "prod", "apps/web")).is_err(), "{arn}");
        }
        assert!(validate(&params(ARN, "", "apps/web")).is_err());
        assert!(validate(&params(ARN, "prod", "")).is_err());
        assert!(validate(&params(ARN, "prod", "a/b/c")).is_err());
        assert!(validate(&params(ARN, "prod", "apps/")).is_err());
    }

    #[test]
    fn accepts_what_the_controller_and_the_cloud_provider_tag() {
        let controller = tags(&[
            (CONTROLLER_CLUSTER_TAG, "prod"),
            (SERVICE_STACK_TAG, "apps/web"),
        ]);
        let ingress = tags(&[
            (CONTROLLER_CLUSTER_TAG, "prod"),
            (INGRESS_STACK_TAG, "apps/web"),
        ]);
        let in_tree = tags(&[
            ("kubernetes.io/cluster/prod", "owned"),
            (SERVICE_NAME_TAG, "apps/web"),
        ]);

        assert!(ownership(&controller, "prod", "apps/web").is_ok());
        assert!(ownership(&ingress, "prod", "apps/web").is_ok());
        assert!(ownership(&in_tree, "prod", "apps/web").is_ok());
    }

    #[test]
    fn refuses_other_clusters_services_and_untagged_resources() {
        let controller = tags(&[
            (CONTROLLER_CLUSTER_TAG, "prod"),
            (SERVICE_STACK_TAG, "apps/web"),
        ]);

        assert!(ownership(&controller, "staging", "apps/web").is_err());
        assert!(ownership(&controller, "prod", "apps/api").is_err());
        // The cluster alone doesn't say which Service it was for.
        assert!(
            ownership(
                &tags(&[(CONTROLLER_CLUSTER_TAG, "prod")]),
                "prod",
                "apps/web"
            )
            .is_err()
        );
        assert!(ownership(&tags(&[]), "prod", "apps/web").is_err());
        // The cloud provider's cluster tag must be `owned`, not `shared`.
        let shared = tags(&[
            ("kubernetes.io/cluster/prod", "shared"),
            (SERVICE_NAME_TAG, "apps/web"),
        ]);
        assert!(ownership(&shared, "prod", "apps/web").is_err());
    }
}

/// The handler against scripted ELBv2 responses, asserting on the changes it sends.
#[cfg(test)]
mod handler_tests {
    use std::sync::{Arc, Mutex};

    use aws_sdk_elasticloadbalancingv2::error::ErrorMetadata;
    use aws_sdk_elasticloadbalancingv2::operation::delete_load_balancer::DeleteLoadBalancerOutput;
    use aws_sdk_elasticloadbalancingv2::operation::delete_target_group::DeleteTargetGroupOutput;
    use aws_sdk_elasticloadbalancingv2::operation::describe_load_balancer_attributes::DescribeLoadBalancerAttributesOutput;
    use aws_sdk_elasticloadbalancingv2::operation::describe_load_balancers::{
        DescribeLoadBalancersError, DescribeLoadBalancersOutput,
    };
    use aws_sdk_elasticloadbalancingv2::operation::describe_tags::DescribeTagsOutput;
    use aws_sdk_elasticloadbalancingv2::operation::describe_target_groups::DescribeTargetGroupsOutput;
    use aws_sdk_elasticloadbalancingv2::operation::describe_target_health::DescribeTargetHealthOutput;
    use aws_sdk_elasticloadbalancingv2::types::{
        LoadBalancerAttribute, LoadBalancerState, LoadBalancerTypeEnum, Tag, TagDescription,
        TargetHealth, TargetHealthDescription,
    };
    use aws_smithy_mocks::{Rule, RuleMode, mock, mock_client};
    use serde_json::{Value, json};

    use super::*;

    const ARN: &str = "arn:aws:elasticloadbalancing:us-east-2:123456789012:loadbalancer/app/k8s-apps-web/0123456789abcdef";
    const CLUSTER: &str = "prod";
    const SERVICE: &str = "apps/web";

    type Pairs = Vec<(&'static str, &'static str)>;

    struct Lb {
        tags: Pairs,
        protected: bool,
        state: LoadBalancerStateEnum,
    }

    struct Group {
        name: &'static str,
        attached: bool,
        tags: Pairs,
        targets: Vec<TargetHealthStateEnum>,
    }

    fn controller_tags(service: &'static str) -> Pairs {
        vec![
            (CONTROLLER_CLUSTER_TAG, CLUSTER),
            (SERVICE_STACK_TAG, service),
        ]
    }

    /// A load balancer the controller created for the Service.
    fn lb() -> Lb {
        Lb {
            tags: controller_tags(SERVICE),
            protected: false,
            state: LoadBalancerStateEnum::Active,
        }
    }

    fn group(name: &'static str, attached: bool, targets: Vec<TargetHealthStateEnum>) -> Group {
        Group {
            name,
            attached,
            tags: controller_tags(SERVICE),
            targets,
        }
    }

    fn group_arn(name: &str) -> String {
        format!(
            "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/{name}/0123456789abcdef"
        )
    }

    fn description(arn: &str, tags: &Pairs) -> TagDescription {
        TagDescription::builder()
            .resource_arn(arn)
            .set_tags(Some(
                tags.iter()
                    .map(|(key, value)| Tag::builder().key(*key).value(*value).build())
                    .collect(),
            ))
            .build()
    }

    /// The ELBv2 calls the handler makes, with the rules that script them.
    struct Elb {
        client: Client,
        delete_lb: Rule,
        delete_tg: Rule,
        /// ARNs of the target groups it deleted.
        deleted: Arc<Mutex<Vec<String>>>,
    }

    impl Elb {
        /// `lb` is the load balancer, if it still exists, and `groups` the target groups of the
        /// region.
        fn new(lb: Option<Lb>, groups: Vec<Group>) -> Self {
            let describe_lb = match &lb {
                Some(lb) => {
                    let state = lb.state.clone();
                    mock!(Client::describe_load_balancers).then_output(move || {
                        DescribeLoadBalancersOutput::builder()
                            .load_balancers(
                                SdkLoadBalancer::builder()
                                    .load_balancer_arn(ARN)
                                    .load_balancer_name("k8s-apps-web")
                                    .r#type(LoadBalancerTypeEnum::Application)
                                    .state(LoadBalancerState::builder().code(state.clone()).build())
                                    .build(),
                            )
                            .build()
                    })
                }
                None => mock!(Client::describe_load_balancers).then_error(|| {
                    DescribeLoadBalancersError::generic(
                        ErrorMetadata::builder()
                            .code("LoadBalancerNotFound")
                            .build(),
                    )
                }),
            };
            let protected = lb.as_ref().is_some_and(|lb| lb.protected);
            let attributes =
                mock!(Client::describe_load_balancer_attributes).then_output(move || {
                    DescribeLoadBalancerAttributesOutput::builder()
                        .attributes(
                            LoadBalancerAttribute::builder()
                                .key(DELETION_PROTECTION_ATTRIBUTE)
                                .value(protected.to_string())
                                .build(),
                        )
                        .build()
                });

            let mut descriptions = Vec::new();
            if let Some(lb) = &lb {
                descriptions.push(description(ARN, &lb.tags));
            }
            for group in &groups {
                descriptions.push(description(&group_arn(group.name), &group.tags));
            }
            let describe_tags = mock!(Client::describe_tags).then_output(move || {
                DescribeTagsOutput::builder()
                    .set_tag_descriptions(Some(descriptions.clone()))
                    .build()
            });

            let sdk_groups: Vec<SdkTargetGroup> = groups
                .iter()
                .map(|g| {
                    SdkTargetGroup::builder()
                        .target_group_arn(group_arn(g.name))
                        .target_group_name(g.name)
                        .port(8080)
                        .set_load_balancer_arns(Some(if g.attached {
                            vec![ARN.to_owned()]
                        } else {
                            vec![]
                        }))
                        .build()
                })
                .collect();
            let describe_groups = mock!(Client::describe_target_groups).then_output(move || {
                DescribeTargetGroupsOutput::builder()
                    .set_target_groups(Some(sdk_groups.clone()))
                    .build()
            });

            let mut rules = vec![describe_lb, attributes, describe_tags, describe_groups];
            for group in &groups {
                let arn = group_arn(group.name);
                let states = group.targets.clone();
                rules.push(
                    mock!(Client::describe_target_health)
                        .match_requests(move |req| req.target_group_arn() == Some(arn.as_str()))
                        .then_output(move || {
                            DescribeTargetHealthOutput::builder()
                                .set_target_health_descriptions(Some(
                                    states
                                        .iter()
                                        .map(|s| {
                                            TargetHealthDescription::builder()
                                                .target_health(
                                                    TargetHealth::builder()
                                                        .state(s.clone())
                                                        .build(),
                                                )
                                                .build()
                                        })
                                        .collect(),
                                ))
                                .build()
                        }),
                );
            }

            // A deletion of any other load balancer matches no rule, which fails the test.
            let delete_lb = mock!(Client::delete_load_balancer)
                .match_requests(|req| req.load_balancer_arn() == Some(ARN))
                .then_output(|| DeleteLoadBalancerOutput::builder().build());
            let deleted = Arc::new(Mutex::new(Vec::new()));
            let recorder = deleted.clone();
            let delete_tg = mock!(Client::delete_target_group).then_compute_output(move |req| {
                recorder
                    .lock()
                    .unwrap()
                    .push(req.target_group_arn().unwrap_or_default().to_owned());
                DeleteTargetGroupOutput::builder().build()
            });
            rules.extend([delete_lb.clone(), delete_tg.clone()]);

            let client = mock_client!(aws_sdk_elasticloadbalancingv2, RuleMode::MatchAny, &rules);
            Self {
                client,
                delete_lb,
                delete_tg,
                deleted,
            }
        }

        async fn call(&self, action: &str) -> Value {
            let req = serde_json::from_value(json!({
                "action": action,
                "loadBalancerArn": ARN,
                "clusterName": CLUSTER,
                "serviceName": SERVICE,
            }))
            .unwrap();
            serde_json::to_value(handle(&self.client, req).await.unwrap()).unwrap()
        }

        /// How many load balancers and target groups the function deleted.
        fn writes(&self) -> (usize, usize) {
            (self.delete_lb.num_calls(), self.delete_tg.num_calls())
        }
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
    async fn plan_lists_what_would_be_deleted_without_deleting_it() {
        let elb = Elb::new(Some(lb()), vec![group("web-http", true, vec![])]);

        let resp = elb.call("plan").await;

        assert_eq!(resp["outcome"], "planned");
        assert_eq!(resp["result"]["loadBalancer"]["name"], "k8s-apps-web");
        assert_eq!(resp["result"]["targetGroups"][0]["name"], "web-http");
        assert_eq!(resp["result"]["targetGroups"][0]["attached"], true);
        assert_eq!(resp["result"]["remaining"], true);
        assert_eq!(elb.writes(), (0, 0));
    }

    #[tokio::test]
    async fn deletes_the_load_balancer_first_and_leaves_its_target_groups() {
        let elb = Elb::new(Some(lb()), vec![group("web-http", true, vec![])]);

        let resp = elb.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["loadBalancerDeleted"], true);
        assert_eq!(
            resp["result"]["remaining"], true,
            "its target groups are next"
        );
        assert_eq!(elb.writes(), (1, 0));
    }

    #[tokio::test]
    async fn is_finished_when_the_load_balancer_has_no_target_groups() {
        let elb = Elb::new(Some(lb()), vec![]);

        let resp = elb.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["remaining"], false);
        assert_eq!(elb.writes(), (1, 0));
    }

    #[tokio::test]
    async fn never_deletes_a_load_balancer_that_has_live_targets() {
        for targets in [
            vec![TargetHealthStateEnum::Healthy],
            vec![TargetHealthStateEnum::Initial],
            vec![
                TargetHealthStateEnum::Unhealthy,
                TargetHealthStateEnum::Healthy,
            ],
        ] {
            let elb = Elb::new(Some(lb()), vec![group("web-http", true, targets.clone())]);

            let resp = elb.call("execute").await;

            assert_eq!(resp["outcome"], "refused", "{targets:?}");
            assert_eq!(failed(&resp), ["no-healthy-targets"], "{targets:?}");
            assert_eq!(elb.writes(), (0, 0), "{targets:?}");
        }
    }

    #[tokio::test]
    async fn deletes_when_no_target_is_receiving_traffic() {
        let targets = vec![
            TargetHealthStateEnum::Unhealthy,
            TargetHealthStateEnum::Unused,
            TargetHealthStateEnum::Draining,
        ];
        let elb = Elb::new(Some(lb()), vec![group("web-http", true, targets)]);

        let resp = elb.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(elb.writes(), (1, 0));
    }

    #[tokio::test]
    async fn never_deletes_a_load_balancer_of_another_service_or_cluster() {
        for tags in [
            controller_tags("apps/api"),
            vec![
                (CONTROLLER_CLUSTER_TAG, "staging"),
                (SERVICE_STACK_TAG, SERVICE),
            ],
            vec![(CONTROLLER_CLUSTER_TAG, CLUSTER)],
            vec![],
        ] {
            let other = Lb {
                tags: tags.clone(),
                ..lb()
            };
            let elb = Elb::new(Some(other), vec![]);

            let resp = elb.call("execute").await;

            assert_eq!(resp["outcome"], "refused", "{tags:?}");
            assert_eq!(failed(&resp), ["kubernetes"], "{tags:?}");
            assert_eq!(elb.writes(), (0, 0), "{tags:?}");
        }
    }

    #[tokio::test]
    async fn accepts_a_load_balancer_of_the_kubernetes_cloud_provider() {
        let in_tree = Lb {
            tags: vec![
                ("kubernetes.io/cluster/prod", "owned"),
                (SERVICE_NAME_TAG, SERVICE),
            ],
            ..lb()
        };
        let elb = Elb::new(Some(in_tree), vec![]);

        let resp = elb.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(elb.writes(), (1, 0));
    }

    #[tokio::test]
    async fn never_deletes_protected_or_provisioning_load_balancers() {
        let protected = Lb {
            protected: true,
            ..lb()
        };
        let elb = Elb::new(Some(protected), vec![]);
        let resp = elb.call("execute").await;
        assert_eq!(failed(&resp), ["deletion-protection"]);
        assert_eq!(elb.writes(), (0, 0));

        let provisioning = Lb {
            state: LoadBalancerStateEnum::Provisioning,
            ..lb()
        };
        let elb = Elb::new(Some(provisioning), vec![]);
        let resp = elb.call("execute").await;
        assert_eq!(failed(&resp), ["state"]);
        assert_eq!(elb.writes(), (0, 0));
    }

    #[tokio::test]
    async fn deletes_the_target_groups_left_after_the_load_balancer() {
        let other = Group {
            tags: controller_tags("apps/api"),
            ..group("api-http", false, vec![])
        };
        let elb = Elb::new(
            None,
            vec![
                group("web-http", false, vec![]),
                group("web-https", false, vec![TargetHealthStateEnum::Unused]),
                other,
            ],
        );

        let resp = elb.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(
            resp["result"]["deletedTargetGroups"],
            json!(["web-http", "web-https"])
        );
        assert_eq!(resp["result"]["remaining"], false);
        assert_eq!(resp["result"]["loadBalancerDeleted"], false);
        assert_eq!(elb.writes(), (0, 2));
        // Only the target groups created for this Service, not the other Service's.
        assert_eq!(
            *elb.deleted.lock().unwrap(),
            [group_arn("web-http"), group_arn("web-https")]
        );
    }

    #[tokio::test]
    async fn never_deletes_target_groups_in_use_or_with_live_targets() {
        let elb = Elb::new(None, vec![group("web-http", true, vec![])]);
        let resp = elb.call("execute").await;
        assert_eq!(resp["outcome"], "refused");
        assert_eq!(failed(&resp), ["unattached"]);
        assert_eq!(elb.writes(), (0, 0));

        let live = group("web-http", false, vec![TargetHealthStateEnum::Healthy]);
        let elb = Elb::new(None, vec![live, group("web-https", false, vec![])]);
        let resp = elb.call("execute").await;
        assert_eq!(failed(&resp), ["no-healthy-targets"]);
        assert_eq!(elb.writes(), (0, 0), "none is deleted when one is refused");
    }

    #[tokio::test]
    async fn reports_when_nothing_is_left() {
        let other = Group {
            tags: controller_tags("apps/api"),
            ..group("api-http", false, vec![])
        };
        for groups in [vec![], vec![other]] {
            let elb = Elb::new(None, groups);

            let resp = elb.call("execute").await;

            assert_eq!(resp["outcome"], "refused");
            assert_eq!(failed(&resp), ["exists"]);
            assert_eq!(elb.writes(), (0, 0));
        }
    }
}
