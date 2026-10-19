//! Deletes a standalone EC2 instance together with its root volume and network interfaces.
//!
//! Data volumes are kept (their `DeleteOnTermination` is cleared); delete them with
//! volume-delete once they are no longer needed. Instances in an Auto Scaling group and EKS
//! worker nodes are refused: scale the node group instead so the group doesn't replace them,
//! and so are instances with termination protection, which this function never turns off.
//!
//! `execute` first sets delete-on-termination on the root volume and network interfaces (and
//! clears it on data volumes), then terminates the instance. If the instance is still updating
//! after the first step, execute is refused and a later execute terminates it.

use aws_sdk_ec2::Client;
use aws_sdk_ec2::error::ProvideErrorMetadata;
use aws_sdk_ec2::types::{
    EbsInstanceBlockDeviceSpecification, Instance, InstanceAttributeName,
    InstanceBlockDeviceMappingSpecification, InstanceNetworkInterface, InstanceStateName,
    NetworkInterfaceAttachmentChanges,
};
use functions_aws::provider_error;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};

/// Tag Auto Scaling sets on instances it manages.
const ASG_TAG: &str = "aws:autoscaling:groupName";

/// Tag EKS sets on managed node group instances.
const EKS_NODEGROUP_TAG: &str = "eks:nodegroup-name";

/// Prefix of the tags Kubernetes cloud providers set on cluster nodes.
const K8S_CLUSTER_TAG_PREFIX: &str = "kubernetes.io/cluster/";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    instance_id: String,
}

#[derive(Debug, Clone)]
struct InstanceView {
    id: String,
    name: Option<String>,
    state: InstanceStateName,
    asg: Option<String>,
    eks_nodegroup: Option<String>,
    cluster_tag: Option<String>,
    /// Whether the API refuses to terminate the instance (`DisableApiTermination`). Not part of
    /// `DescribeInstances`, so [`load_instance`] reads it separately.
    termination_protected: bool,
    root_volume_id: Option<String>,
    /// Non-root EBS volume IDs.
    data_volumes: Vec<String>,
    /// Every EBS mapping with its current delete-on-termination.
    block_devices: Vec<BlockDeviceView>,
    network_interfaces: Vec<NetworkInterfaceView>,
    /// Whether terminating the instance deletes the root volume and NICs and keeps data volumes.
    delete_on_termination_set: bool,
}

#[derive(Debug, Clone)]
struct BlockDeviceView {
    device_name: String,
    delete_on_termination: Option<bool>,
    is_root: bool,
}

#[derive(Debug, Clone)]
struct NetworkInterfaceView {
    id: String,
    attachment_id: Option<String>,
    delete_on_termination: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceSummary {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_scaling_group: Option<String>,
    /// Deleted with the instance when delete-on-termination is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    os_volume: Option<String>,
    /// Deleted with the instance when delete-on-termination is set.
    network_interfaces: Vec<String>,
    /// Kept after termination.
    data_volumes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    instance: Option<InstanceSummary>,
    /// Whether terminating the instance now also deletes its root volume and network interfaces
    /// and keeps its data volumes.
    delete_on_termination_set: bool,
    deleted: bool,
}

fn validate(params: &Params) -> Result<(), Error> {
    let id = params.instance_id.as_str();
    let valid = id.strip_prefix("i-").is_some_and(|hex| {
        (8..=17).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    });
    if !valid {
        return Err(Error::invalid_request(format!(
            "instanceId {id:?} is not an EC2 instance ID (i-...)"
        )));
    }
    Ok(())
}

async fn handle(ec2: &Client, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    validate(&params)?;

    let instance = load_instance(ec2, &params.instance_id).await?;
    let mut guards = evaluate(instance.as_ref());
    let mut output = Output {
        instance: instance.as_ref().map(summary),
        delete_on_termination_set: instance
            .as_ref()
            .is_some_and(|i| i.delete_on_termination_set),
        deleted: false,
    };

    let (Action::Execute, Some(instance)) = (req.action, instance) else {
        return Ok(match req.action {
            Action::Plan => Response::planned(guards, output),
            Action::Execute => Response::refused(guards).with_result(output),
        });
    };
    if !guards.iter().all(|g| g.passed) {
        return Ok(Response::refused(guards).with_result(output));
    }

    if !instance.delete_on_termination_set {
        set_delete_on_termination(ec2, &instance).await?;
        tracing::info!(instance_id = %instance.id, "set delete-on-termination options");
        output.delete_on_termination_set = true;

        let updated = find_instance(ec2, &instance.id).await?;
        let ready = updated
            .as_ref()
            .is_some_and(|i| i.delete_on_termination_set && is_idle(&i.state));
        output.instance = updated.as_ref().map(summary);
        if !ready {
            guards.push(Guard::fail(
                "delete-on-termination",
                "the instance is updating its delete-on-termination options; execute again once it has finished",
            ));
            return Ok(Response::refused(guards).with_result(output));
        }
    }

    ec2.terminate_instances()
        .instance_ids(&params.instance_id)
        .send()
        .await
        .map_err(provider_error)?;
    tracing::info!(instance_id = %params.instance_id, "terminating instance");
    output.deleted = true;
    Ok(Response::done(guards, output))
}

/// The instance with its termination protection, or `None` if it doesn't exist.
async fn load_instance(ec2: &Client, id: &str) -> Result<Option<InstanceView>, Error> {
    let Some(mut instance) = find_instance(ec2, id).await? else {
        return Ok(None);
    };
    let attribute = ec2
        .describe_instance_attribute()
        .instance_id(id)
        .attribute(InstanceAttributeName::DisableApiTermination)
        .send()
        .await
        .map_err(provider_error)?;
    instance.termination_protected = attribute
        .disable_api_termination()
        .and_then(|v| v.value())
        .unwrap_or(false);
    Ok(Some(instance))
}

async fn find_instance(ec2: &Client, id: &str) -> Result<Option<InstanceView>, Error> {
    match ec2.describe_instances().instance_ids(id).send().await {
        Ok(out) => Ok(out
            .reservations()
            .iter()
            .flat_map(|r| r.instances())
            .next()
            .map(instance_from)),
        Err(err) if err.code() == Some("InvalidInstanceID.NotFound") => Ok(None),
        Err(err) => Err(provider_error(err)),
    }
}

async fn set_delete_on_termination(ec2: &Client, instance: &InstanceView) -> Result<(), Error> {
    let mappings: Vec<_> = instance
        .block_devices
        .iter()
        .map(|bd| {
            InstanceBlockDeviceMappingSpecification::builder()
                .device_name(&bd.device_name)
                .ebs(
                    EbsInstanceBlockDeviceSpecification::builder()
                        .delete_on_termination(bd.is_root)
                        .build(),
                )
                .build()
        })
        .collect();

    if !mappings.is_empty() {
        ec2.modify_instance_attribute()
            .instance_id(&instance.id)
            .set_block_device_mappings(Some(mappings))
            .send()
            .await
            .map_err(provider_error)?;
    }

    for nic in &instance.network_interfaces {
        if nic.delete_on_termination == Some(true) {
            continue;
        }
        let attachment_id = nic.attachment_id.as_deref().ok_or_else(|| {
            Error::provider(format!("network interface {} has no attachment id", nic.id))
        })?;
        ec2.modify_network_interface_attribute()
            .network_interface_id(&nic.id)
            .attachment(
                NetworkInterfaceAttachmentChanges::builder()
                    .attachment_id(attachment_id)
                    .delete_on_termination(true)
                    .build(),
            )
            .send()
            .await
            .map_err(provider_error)?;
    }

    Ok(())
}

fn instance_from(inst: &Instance) -> InstanceView {
    let tag = |key: &str| {
        inst.tags()
            .iter()
            .find(|t| t.key() == Some(key))
            .and_then(|t| t.value())
            .map(str::to_owned)
    };
    let cluster_tag = inst.tags().iter().find_map(|t| {
        let key = t.key()?;
        key.starts_with(K8S_CLUSTER_TAG_PREFIX)
            .then(|| key.to_owned())
    });

    let root_device_name = inst.root_device_name().map(str::to_owned);
    let mut root_volume_id = None;
    let mut data_volumes = Vec::new();
    let mut block_devices = Vec::new();

    for bdm in inst.block_device_mappings() {
        let Some(ebs) = bdm.ebs() else {
            continue;
        };
        let Some(device_name) = bdm.device_name() else {
            continue;
        };
        let volume_id = ebs.volume_id().unwrap_or_default().to_owned();
        let is_root = Some(device_name) == root_device_name.as_deref();
        if is_root {
            root_volume_id = Some(volume_id.clone());
        } else {
            data_volumes.push(volume_id);
        }
        block_devices.push(BlockDeviceView {
            device_name: device_name.to_owned(),
            delete_on_termination: ebs.delete_on_termination(),
            is_root,
        });
    }

    let network_interfaces: Vec<_> = inst
        .network_interfaces()
        .iter()
        .filter_map(network_interface_from)
        .collect();

    let root_ok = block_devices
        .iter()
        .find(|bd| bd.is_root)
        .is_some_and(|bd| bd.delete_on_termination == Some(true));
    let data_ok = block_devices
        .iter()
        .filter(|bd| !bd.is_root)
        .all(|bd| bd.delete_on_termination == Some(false));
    let nics_ok = !network_interfaces.is_empty()
        && network_interfaces
            .iter()
            .all(|n| n.delete_on_termination == Some(true));
    let delete_on_termination_set = root_volume_id.is_some() && root_ok && data_ok && nics_ok;

    InstanceView {
        id: inst.instance_id().unwrap_or_default().to_owned(),
        name: tag("Name"),
        state: inst
            .state()
            .and_then(|s| s.name().cloned())
            .unwrap_or(InstanceStateName::Pending),
        asg: tag(ASG_TAG),
        eks_nodegroup: tag(EKS_NODEGROUP_TAG),
        cluster_tag,
        termination_protected: false,
        root_volume_id,
        data_volumes,
        block_devices,
        network_interfaces,
        delete_on_termination_set,
    }
}

fn network_interface_from(nic: &InstanceNetworkInterface) -> Option<NetworkInterfaceView> {
    Some(NetworkInterfaceView {
        id: nic.network_interface_id()?.to_owned(),
        attachment_id: nic
            .attachment()
            .and_then(|a| a.attachment_id())
            .map(str::to_owned),
        delete_on_termination: nic.attachment().and_then(|a| a.delete_on_termination()),
    })
}

fn evaluate(instance: Option<&InstanceView>) -> Vec<Guard> {
    let Some(instance) = instance else {
        return vec![Guard::fail("exists", "instance not found")];
    };
    let state = instance.state.as_str();
    vec![
        Guard::pass("exists", format!("instance {}", instance.id)),
        match &instance.asg {
            Some(asg) => Guard::fail(
                "standalone",
                format!(
                    "belongs to Auto Scaling group {asg}; scale it instead so it doesn't replace the instance"
                ),
            ),
            None => Guard::pass("standalone", "not part of an Auto Scaling group"),
        },
        match (&instance.eks_nodegroup, &instance.cluster_tag) {
            (Some(ng), _) => Guard::fail(
                "not-eks",
                format!("managed by EKS node group {ng}; use node-pool-resize instead"),
            ),
            (_, Some(tag)) => Guard::fail(
                "not-eks",
                format!("managed by Kubernetes (tag {tag}); use node-pool-resize instead"),
            ),
            _ => Guard::pass("not-eks", "not managed by EKS or Kubernetes"),
        },
        Guard::check(
            "deletion-protection",
            !instance.termination_protected,
            if instance.termination_protected {
                "termination protection is enabled; disable it first"
            } else {
                "termination protection is off"
            },
        ),
        Guard::check(
            "managed-os-volume",
            instance.root_volume_id.is_some(),
            "the root volume must be an EBS volume to be deleted with the instance",
        ),
        Guard::check(
            "idle",
            is_idle(&instance.state),
            format!("instance state {state}"),
        ),
    ]
}

fn is_idle(state: &InstanceStateName) -> bool {
    matches!(
        state,
        InstanceStateName::Running | InstanceStateName::Stopped
    )
}

fn summary(instance: &InstanceView) -> InstanceSummary {
    InstanceSummary {
        id: instance.id.clone(),
        name: instance.name.clone(),
        state: instance.state.as_str().to_owned(),
        auto_scaling_group: instance.asg.clone(),
        os_volume: instance.root_volume_id.clone(),
        network_interfaces: instance
            .network_interfaces
            .iter()
            .map(|n| n.id.clone())
            .collect(),
        data_volumes: instance.data_volumes.clone(),
    }
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let ec2 = Client::new(&functions_aws::sdk_config().await);

    functions_aws::run(move |req| {
        let ec2 = ec2.clone();
        async move { handle(&ec2, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use aws_sdk_ec2::types::{
        EbsInstanceBlockDevice, GroupIdentifier, InstanceBlockDeviceMapping,
        InstanceNetworkInterfaceAssociation, InstanceNetworkInterfaceAttachment, InstanceState,
        Tag,
    };

    use super::*;

    fn tag(key: &str, value: &str) -> Tag {
        Tag::builder().key(key).value(value).build()
    }

    fn params(id: &str) -> Params {
        Params {
            instance_id: id.into(),
        }
    }

    fn ebs(volume_id: &str, delete: bool) -> EbsInstanceBlockDevice {
        EbsInstanceBlockDevice::builder()
            .volume_id(volume_id)
            .delete_on_termination(delete)
            .build()
    }

    fn bdm(device: &str, volume_id: &str, delete: bool) -> InstanceBlockDeviceMapping {
        InstanceBlockDeviceMapping::builder()
            .device_name(device)
            .ebs(ebs(volume_id, delete))
            .build()
    }

    fn nic(id: &str, delete: bool) -> InstanceNetworkInterface {
        InstanceNetworkInterface::builder()
            .network_interface_id(id)
            .groups(GroupIdentifier::builder().group_id("sg-1").build())
            .attachment(
                InstanceNetworkInterfaceAttachment::builder()
                    .attachment_id("eni-attach-1")
                    .delete_on_termination(delete)
                    .build(),
            )
            .association(
                InstanceNetworkInterfaceAssociation::builder()
                    .public_ip("1.2.3.4")
                    .build(),
            )
            .build()
    }

    fn instance_builder() -> aws_sdk_ec2::types::builders::InstanceBuilder {
        Instance::builder()
            .instance_id("i-0123456789abcdef0")
            .root_device_name("/dev/xvda")
            .state(
                InstanceState::builder()
                    .name(InstanceStateName::Running)
                    .build(),
            )
    }

    fn raw_instance() -> Instance {
        instance_builder()
            .tags(tag("Name", "app-1"))
            .block_device_mappings(bdm("/dev/xvda", "vol-root", false))
            .block_device_mappings(bdm("/dev/sdf", "vol-data", true))
            .network_interfaces(nic("eni-1", false))
            .build()
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    #[test]
    fn validates_instance_ids() {
        assert!(validate(&params("i-0123456789abcdef0")).is_ok());
        assert!(validate(&params("i-12345678")).is_ok());
        for id in [
            "",
            "i-",
            "i-xyz12345",
            "vol-0123456789abcdef0",
            "i-0123456789abcdef01",
        ] {
            assert!(validate(&params(id)).is_err(), "{id} should be rejected");
        }
    }

    #[test]
    fn allows_standalone_instances() {
        let view = instance_from(&raw_instance());
        assert!(failed(&evaluate(Some(&view))).is_empty());
        assert!(!view.delete_on_termination_set);
        assert_eq!(failed(&evaluate(None)), ["exists"]);
    }

    #[test]
    fn refuses_asg_and_eks_instances() {
        let view = instance_from(
            &instance_builder()
                .tags(tag(ASG_TAG, "eks-workers"))
                .tags(tag(EKS_NODEGROUP_TAG, "apps"))
                .block_device_mappings(bdm("/dev/xvda", "vol-root", true))
                .network_interfaces(nic("eni-1", true))
                .build(),
        );

        assert_eq!(failed(&evaluate(Some(&view))), ["standalone", "not-eks"]);
    }

    #[test]
    fn refuses_kubernetes_cluster_tagged_instances() {
        let view = instance_from(
            &instance_builder()
                .tags(tag("kubernetes.io/cluster/prod", "owned"))
                .block_device_mappings(bdm("/dev/xvda", "vol-root", true))
                .network_interfaces(nic("eni-1", true))
                .build(),
        );

        assert_eq!(failed(&evaluate(Some(&view))), ["not-eks"]);
    }

    #[test]
    fn refuses_instance_store_root_and_busy_instances() {
        let no_ebs = instance_builder()
            .state(
                InstanceState::builder()
                    .name(InstanceStateName::Pending)
                    .build(),
            )
            .network_interfaces(nic("eni-1", true))
            .build();

        assert_eq!(
            failed(&evaluate(Some(&instance_from(&no_ebs)))),
            ["managed-os-volume", "idle"]
        );
    }

    #[test]
    fn detects_delete_on_termination_options() {
        let ready = instance_builder()
            .state(
                InstanceState::builder()
                    .name(InstanceStateName::Stopped)
                    .build(),
            )
            .block_device_mappings(bdm("/dev/xvda", "vol-root", true))
            .block_device_mappings(bdm("/dev/sdf", "vol-data", false))
            .network_interfaces(nic("eni-1", true))
            .build();

        let view = instance_from(&ready);
        assert!(view.delete_on_termination_set);
        assert_eq!(view.data_volumes, ["vol-data"]);
    }

    #[test]
    fn summarizes_what_is_deleted_and_kept() {
        let s = summary(&instance_from(&raw_instance()));

        assert_eq!(s.os_volume.as_deref(), Some("vol-root"));
        assert_eq!(s.network_interfaces, ["eni-1"]);
        assert_eq!(s.data_volumes, ["vol-data"]);
        assert_eq!(s.name.as_deref(), Some("app-1"));
    }

    #[test]
    fn refuses_instances_with_termination_protection() {
        let mut view = instance_from(&raw_instance());
        view.termination_protected = true;

        assert_eq!(failed(&evaluate(Some(&view))), ["deletion-protection"]);
    }

    #[test]
    fn refuses_transitional_states() {
        for state in [
            InstanceStateName::Pending,
            InstanceStateName::Stopping,
            InstanceStateName::ShuttingDown,
            InstanceStateName::Terminated,
        ] {
            let view = instance_from(
                &instance_builder()
                    .state(InstanceState::builder().name(state.clone()).build())
                    .block_device_mappings(bdm("/dev/xvda", "vol-root", true))
                    .network_interfaces(nic("eni-1", true))
                    .build(),
            );

            assert_eq!(failed(&evaluate(Some(&view))), ["idle"], "{state:?}");
        }
    }
}

/// The handler against scripted EC2 responses, asserting on the changes it sends.
#[cfg(test)]
mod handler_tests {
    use aws_sdk_ec2::error::ErrorMetadata;
    use aws_sdk_ec2::operation::describe_instance_attribute::DescribeInstanceAttributeOutput;
    use aws_sdk_ec2::operation::describe_instances::{
        DescribeInstancesError, DescribeInstancesOutput,
    };
    use aws_sdk_ec2::operation::modify_instance_attribute::ModifyInstanceAttributeOutput;
    use aws_sdk_ec2::operation::modify_network_interface_attribute::ModifyNetworkInterfaceAttributeOutput;
    use aws_sdk_ec2::operation::terminate_instances::TerminateInstancesOutput;
    use aws_sdk_ec2::types::{
        AttributeBooleanValue, EbsInstanceBlockDevice, InstanceBlockDeviceMapping,
        InstanceNetworkInterfaceAttachment, InstanceState, Reservation, Tag,
    };
    use aws_smithy_mocks::{Rule, RuleMode, mock, mock_client};
    use serde_json::{Value, json};

    use super::*;

    const ID: &str = "i-0123456789abcdef0";

    /// A running instance with a root volume, a data volume and a network interface, each with
    /// the given delete-on-termination: `false` for everything but the data volume is the
    /// opposite of what deleting needs, `true` for the root volume and NIC and `false` for the
    /// data volume is what the function sets.
    fn instance(options_set: bool, tags: &[(&str, &str)]) -> Instance {
        let (root, data, nic) = if options_set {
            (true, false, true)
        } else {
            (false, true, false)
        };
        let mut builder = Instance::builder()
            .instance_id(ID)
            .root_device_name("/dev/xvda")
            .state(
                InstanceState::builder()
                    .name(InstanceStateName::Running)
                    .build(),
            )
            .block_device_mappings(mapping("/dev/xvda", "vol-root", root))
            .block_device_mappings(mapping("/dev/sdf", "vol-data", data))
            .network_interfaces(
                InstanceNetworkInterface::builder()
                    .network_interface_id("eni-1")
                    .attachment(
                        InstanceNetworkInterfaceAttachment::builder()
                            .attachment_id("eni-attach-1")
                            .delete_on_termination(nic)
                            .build(),
                    )
                    .build(),
            );
        for (key, value) in tags {
            builder = builder.tags(Tag::builder().key(*key).value(*value).build());
        }
        builder.build()
    }

    fn mapping(device: &str, volume: &str, delete: bool) -> InstanceBlockDeviceMapping {
        InstanceBlockDeviceMapping::builder()
            .device_name(device)
            .ebs(
                EbsInstanceBlockDevice::builder()
                    .volume_id(volume)
                    .delete_on_termination(delete)
                    .build(),
            )
            .build()
    }

    /// The EC2 calls the handler makes, with the rules that script them.
    struct Ec2 {
        client: Client,
        modify: Rule,
        nic: Rule,
        terminate: Rule,
    }

    impl Ec2 {
        /// `reads` are the answers to successive DescribeInstances calls.
        fn new(reads: Vec<Instance>, protected: bool) -> Self {
            let mut describe = mock!(Client::describe_instances).sequence();
            for read in reads {
                describe = describe.output(move || {
                    DescribeInstancesOutput::builder()
                        .reservations(Reservation::builder().instances(read.clone()).build())
                        .build()
                });
            }
            let describe = describe.build();
            let attribute = mock!(Client::describe_instance_attribute).then_output(move || {
                DescribeInstanceAttributeOutput::builder()
                    .disable_api_termination(
                        AttributeBooleanValue::builder().value(protected).build(),
                    )
                    .build()
            });
            // Matching on the request makes a call with other contents fail the test.
            let modify = mock!(Client::modify_instance_attribute)
                .match_requests(|req| {
                    let devices = req.block_device_mappings();
                    let deletes = |device: &str| {
                        devices
                            .iter()
                            .find(|d| d.device_name() == Some(device))
                            .and_then(|d| d.ebs())
                            .and_then(|e| e.delete_on_termination())
                    };
                    req.instance_id() == Some(ID)
                        && deletes("/dev/xvda") == Some(true)
                        && deletes("/dev/sdf") == Some(false)
                })
                .then_output(|| ModifyInstanceAttributeOutput::builder().build());
            let nic = mock!(Client::modify_network_interface_attribute)
                .match_requests(|req| {
                    req.network_interface_id() == Some("eni-1")
                        && req.attachment().and_then(|a| a.attachment_id()) == Some("eni-attach-1")
                        && req.attachment().and_then(|a| a.delete_on_termination()) == Some(true)
                })
                .then_output(|| ModifyNetworkInterfaceAttributeOutput::builder().build());
            let terminate = mock!(Client::terminate_instances)
                .match_requests(|req| req.instance_ids() == [ID])
                .then_output(|| TerminateInstancesOutput::builder().build());

            let client = mock_client!(
                aws_sdk_ec2,
                RuleMode::MatchAny,
                [&describe, &attribute, &modify, &nic, &terminate]
            );
            Self {
                client,
                modify,
                nic,
                terminate,
            }
        }

        async fn call(&self, action: &str) -> Value {
            let req = serde_json::from_value(json!({"action": action, "instanceId": ID})).unwrap();
            serde_json::to_value(handle(&self.client, req).await.unwrap()).unwrap()
        }

        /// How many times the function changed the instance, its NICs and terminated it.
        fn writes(&self) -> (usize, usize, usize) {
            (
                self.modify.num_calls(),
                self.nic.num_calls(),
                self.terminate.num_calls(),
            )
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
    async fn plan_reads_only() {
        let ec2 = Ec2::new(vec![instance(false, &[])], false);

        let resp = ec2.call("plan").await;

        assert_eq!(resp["outcome"], "planned");
        assert_eq!(resp["result"]["deleteOnTerminationSet"], false);
        assert_eq!(
            resp["result"]["instance"]["dataVolumes"],
            json!(["vol-data"])
        );
        assert_eq!(ec2.writes(), (0, 0, 0));
    }

    #[tokio::test]
    async fn sets_delete_options_then_terminates_in_one_execute() {
        let ec2 = Ec2::new(vec![instance(false, &[]), instance(true, &[])], false);

        let resp = ec2.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["deleted"], true);
        assert_eq!(resp["result"]["deleteOnTerminationSet"], true);
        // The requests were matched on their contents: the root volume is deleted with the
        // instance, the data volume kept and the network interface deleted.
        assert_eq!(ec2.writes(), (1, 1, 1));
    }

    #[tokio::test]
    async fn terminates_directly_once_the_options_are_set() {
        let ec2 = Ec2::new(vec![instance(true, &[])], false);

        let resp = ec2.call("execute").await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(ec2.writes(), (0, 0, 1));
    }

    #[tokio::test]
    async fn waits_when_the_options_are_not_applied_yet() {
        // The second read still shows the old options.
        let ec2 = Ec2::new(vec![instance(false, &[]), instance(false, &[])], false);

        let resp = ec2.call("execute").await;

        assert_eq!(resp["outcome"], "refused");
        assert_eq!(failed(&resp), ["delete-on-termination"]);
        assert_eq!(resp["result"]["deleted"], false);
        assert_eq!(ec2.writes(), (1, 1, 0), "no terminate while updating");
    }

    #[tokio::test]
    async fn never_changes_a_protected_instance() {
        // One read for each call below.
        let ec2 = Ec2::new(vec![instance(false, &[]), instance(false, &[])], true);

        for action in ["plan", "execute"] {
            let resp = ec2.call(action).await;

            assert_eq!(resp["outcome"], "refused", "{action}");
            assert_eq!(failed(&resp), ["deletion-protection"], "{action}");
        }
        assert_eq!(ec2.writes(), (0, 0, 0));
    }

    #[tokio::test]
    async fn never_touches_auto_scaling_and_eks_instances() {
        for tags in [
            vec![(ASG_TAG, "workers")],
            vec![(EKS_NODEGROUP_TAG, "apps")],
        ] {
            let ec2 = Ec2::new(vec![instance(false, &tags)], false);

            let resp = ec2.call("execute").await;

            assert_eq!(resp["outcome"], "refused", "{tags:?}");
            assert_eq!(ec2.writes(), (0, 0, 0), "{tags:?}");
        }
    }

    #[tokio::test]
    async fn reports_a_missing_instance() {
        let describe = mock!(Client::describe_instances).then_error(|| {
            DescribeInstancesError::generic(
                ErrorMetadata::builder()
                    .code("InvalidInstanceID.NotFound")
                    .build(),
            )
        });
        let client = mock_client!(aws_sdk_ec2, RuleMode::MatchAny, [&describe]);
        let req = serde_json::from_value(json!({"action": "execute", "instanceId": ID})).unwrap();

        let resp = serde_json::to_value(handle(&client, req).await.unwrap()).unwrap();

        assert_eq!(resp["outcome"], "refused");
        assert_eq!(failed(&resp), ["exists"]);
    }
}
