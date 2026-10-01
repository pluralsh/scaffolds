//! Deletes a standalone EC2 instance together with its root volume and network interfaces.
//!
//! Data volumes are kept (their `DeleteOnTermination` is cleared); delete them with
//! volume-delete once they are no longer needed. Instances in an Auto Scaling group and EKS
//! worker nodes are refused: scale the node group instead so the group doesn't replace them.
//!
//! `execute` first sets delete-on-termination on the root volume and network interfaces (and
//! clears it on data volumes), then terminates the instance. If the instance is still updating
//! after the first step, execute is refused and a later execute terminates it.

use aws_sdk_ec2::Client;
use aws_sdk_ec2::error::ProvideErrorMetadata;
use aws_sdk_ec2::types::{
    EbsInstanceBlockDeviceSpecification, Instance, InstanceBlockDeviceMappingSpecification,
    InstanceNetworkInterface, InstanceStateName, NetworkInterfaceAttachmentChanges,
};
use functions_aws::provider_error;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};

/// Tag Auto Scaling sets on instances it manages.
const ASG_TAG: &str = "aws:autoscaling:groupName";

/// Tag EKS sets on managed node group instances.
const EKS_NODEGROUP_TAG: &str = "eks:nodegroup-name";

/// Prefix of the tags the in-tree / cloud provider sets on cluster nodes.
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
    root_volume_id: Option<String>,
    /// Non-root EBS volume IDs.
    data_volumes: Vec<String>,
    /// Device name and desired delete-on-termination for every EBS mapping.
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

    let instance = find_instance(ec2, &params.instance_id).await?;
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
        InstanceStateName::Running | InstanceStateName::Stopping | InstanceStateName::Stopped
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
}
