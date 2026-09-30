//! Deletes a standalone Azure VM together with its OS disk and network interfaces.
//!
//! Data disks are detached and kept; delete them with volume-delete once they are no longer
//! needed. VMs of scale sets, which includes AKS nodes, are refused: scale their node pool
//! with node-pool-resize instead, so the scale set doesn't replace them.
//!
//! Azure deletes a VM's disks and network interfaces with it according to their
//! `deleteOption`, so execute first sets the OS disk and network interfaces to `Delete` and the
//! data disks to `Detach`, and then deletes the VM. If the VM is still updating after the first
//! step, execute is refused and a later execute deletes it.

use std::collections::HashMap;
use std::sync::Arc;

use functions_azure::ManagedIdentityCredential;
use functions_azure::arm::{self, Arm, Precondition, ResourceId};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const VM_API_VERSION: &str = "2024-07-01";

/// Prefix of the tags AKS sets on the VMs and scale sets it manages.
const AKS_TAG_PREFIX: &str = "aks-managed-";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    vm_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Vm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    properties: VmProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VmProperties {
    #[serde(default)]
    provisioning_state: String,
    #[serde(default)]
    virtual_machine_scale_set: Option<SubResource>,
    #[serde(default)]
    storage_profile: StorageProfile,
    #[serde(default)]
    network_profile: NetworkProfile,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageProfile {
    #[serde(default)]
    os_disk: Option<OsDisk>,
    #[serde(default)]
    data_disks: Vec<DataDisk>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OsDisk {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    managed_disk: Option<SubResource>,
    #[serde(default)]
    delete_option: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataDisk {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    delete_option: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkProfile {
    #[serde(default)]
    network_interfaces: Vec<NicRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NicRef {
    #[serde(default)]
    id: String,
    #[serde(default)]
    properties: Option<NicRefProperties>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NicRefProperties {
    #[serde(default)]
    delete_option: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SubResource {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VmSummary {
    name: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scale_set: Option<String>,
    /// Deleted with the VM.
    #[serde(skip_serializing_if = "Option::is_none")]
    os_disk: Option<String>,
    /// Deleted with the VM.
    network_interfaces: Vec<String>,
    /// Detached and kept.
    data_disks: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    vm: Option<VmSummary>,
    /// Whether deleting the VM, by this function or anyone else, now also deletes its OS disk
    /// and network interfaces.
    delete_options_set: bool,
    deleted: bool,
}

async fn handle(
    client: &reqwest::Client,
    credential: &ManagedIdentityCredential,
    req: Request<Params>,
) -> Result<Response<Output>, Error> {
    let id = ResourceId::parse(&req.params.vm_id, &[arm::VIRTUAL_MACHINE]).ok_or_else(|| {
        Error::invalid_request(format!(
            "vmId {:?} is not a virtual machine resource ID (/subscriptions/<id>/resourceGroups/<group>/providers/Microsoft.Compute/virtualMachines/<name>)",
            req.params.vm_id
        ))
    })?;
    let path = id.id();

    let arm = Arm::connect(client.clone(), credential).await?;
    let raw: Option<Value> = arm.get(&path, VM_API_VERSION).await?;
    let vm = raw.as_ref().map(parse_vm).transpose()?;
    let mut guards = evaluate(vm.as_ref());
    let mut output = Output {
        vm: vm.as_ref().map(summary),
        delete_options_set: vm.as_ref().is_some_and(delete_options_set),
        deleted: false,
    };
    let (Action::Execute, Some(raw), Some(vm)) = (req.action, raw, vm) else {
        return Ok(match req.action {
            Action::Plan => Response::planned(guards, output),
            Action::Execute => Response::refused(guards).with_result(output),
        });
    };
    if !guards.iter().all(|g| g.passed) {
        return Ok(Response::refused(guards).with_result(output));
    }

    let mut etag = vm.etag.clone();
    if !delete_options_set(&vm) {
        // The lists in the body replace the VM's, so a VM changed since it was read is left alone.
        let updated = arm
            .patch(
                &path,
                VM_API_VERSION,
                &delete_options_body(&raw),
                Precondition::IfMatch(vm.etag.as_deref()),
            )
            .await?;
        tracing::info!(vm = %path, "set delete options");
        output.delete_options_set = true;
        let updated = serde_json::from_value::<Vm>(updated).ok();
        let ready = updated.as_ref().is_some_and(|vm| {
            vm.properties.provisioning_state == "Succeeded" && delete_options_set(vm)
        });
        etag = updated.and_then(|vm| vm.etag);
        if !ready {
            guards.push(Guard::fail(
                "delete-options",
                "the VM is updating its delete options; execute again once it has finished",
            ));
            return Ok(Response::refused(guards).with_result(output));
        }
    }
    arm.delete(
        &path,
        VM_API_VERSION,
        Precondition::IfMatch(etag.as_deref()),
    )
    .await?;
    tracing::info!(vm = %path, "deleting VM");
    output.deleted = true;
    Ok(Response::done(guards, output))
}

fn parse_vm(raw: &Value) -> Result<Vm, Error> {
    serde_json::from_value(raw.clone())
        .map_err(|err| Error::provider(format!("ARM response: {err}")))
}

fn evaluate(vm: Option<&Vm>) -> Vec<Guard> {
    let Some(vm) = vm else {
        return vec![Guard::fail("exists", "VM not found")];
    };
    let props = &vm.properties;
    let aks_tag = vm
        .tags
        .keys()
        .find(|k| k.to_ascii_lowercase().starts_with(AKS_TAG_PREFIX));
    let state = props.provisioning_state.as_str();
    vec![
        Guard::pass("exists", format!("VM {}", vm.name)),
        match &props.virtual_machine_scale_set {
            Some(set) => Guard::fail(
                "standalone",
                format!(
                    "belongs to scale set {}; scale it instead so it doesn't replace the VM",
                    last_segment(&set.id)
                ),
            ),
            None => Guard::pass("standalone", "not part of a scale set"),
        },
        match aks_tag {
            Some(tag) => Guard::fail(
                "not-aks",
                format!("managed by AKS (tag {tag}); use node-pool-resize instead"),
            ),
            None => Guard::pass("not-aks", "not managed by AKS"),
        },
        Guard::check(
            "managed-os-disk",
            props
                .storage_profile
                .os_disk
                .as_ref()
                .is_some_and(|d| d.managed_disk.is_some()),
            "the OS disk must be a managed disk to be deleted with the VM",
        ),
        Guard::check(
            "idle",
            matches!(state, "Succeeded" | "Failed"),
            format!("provisioning state {state}"),
        ),
    ]
}

/// Whether deleting the VM also deletes its OS disk and network interfaces and keeps its
/// data disks.
fn delete_options_set(vm: &Vm) -> bool {
    let storage = &vm.properties.storage_profile;
    let delete = |option: &Option<String>| option.as_deref() == Some("Delete");
    storage
        .os_disk
        .as_ref()
        .is_some_and(|d| delete(&d.delete_option))
        && storage.data_disks.iter().all(|d| !delete(&d.delete_option))
        && vm
            .properties
            .network_profile
            .network_interfaces
            .iter()
            .all(|n| {
                n.properties
                    .as_ref()
                    .is_some_and(|p| delete(&p.delete_option))
            })
}

/// PATCH body setting the OS disk and network interfaces to be deleted with the VM and the
/// data disks to be detached. Lists replace the VM's lists, so they are sent as read, with
/// only `deleteOption` changed.
fn delete_options_body(raw: &Value) -> Value {
    let list = |pointer: &str| {
        raw.pointer(pointer)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut data_disks = list("/properties/storageProfile/dataDisks");
    for disk in &mut data_disks {
        disk["deleteOption"] = json!("Detach");
    }
    let mut nics = list("/properties/networkProfile/networkInterfaces");
    for nic in &mut nics {
        if !nic.get("properties").is_some_and(Value::is_object) {
            nic["properties"] = json!({});
        }
        nic["properties"]["deleteOption"] = json!("Delete");
    }
    json!({
        "properties": {
            "storageProfile": {
                "osDisk": { "deleteOption": "Delete" },
                "dataDisks": data_disks,
            },
            "networkProfile": { "networkInterfaces": nics },
        }
    })
}

fn summary(vm: &Vm) -> VmSummary {
    let props = &vm.properties;
    VmSummary {
        name: vm.name.clone(),
        state: props.provisioning_state.clone(),
        scale_set: props
            .virtual_machine_scale_set
            .as_ref()
            .map(|s| last_segment(&s.id).to_owned()),
        os_disk: props
            .storage_profile
            .os_disk
            .as_ref()
            .and_then(|d| d.name.clone()),
        network_interfaces: props
            .network_profile
            .network_interfaces
            .iter()
            .map(|n| last_segment(&n.id).to_owned())
            .collect(),
        data_disks: props
            .storage_profile
            .data_disks
            .iter()
            .filter_map(|d| d.name.clone())
            .collect(),
    }
}

fn last_segment(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
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
    use super::*;

    const NIC: &str =
        "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/vm-1-nic";

    fn raw_vm() -> Value {
        json!({
            "name": "vm-1",
            "tags": {"env": "dev"},
            "properties": {
                "provisioningState": "Succeeded",
                "storageProfile": {
                    "osDisk": {"name": "vm-1-os", "osType": "Linux", "createOption": "FromImage",
                               "managedDisk": {"id": "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/vm-1-os"},
                               "deleteOption": "Detach"},
                    "dataDisks": [{"lun": 0, "name": "vm-1-data", "createOption": "Attach",
                                   "managedDisk": {"id": "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/vm-1-data"},
                                   "deleteOption": "Delete"}]
                },
                "networkProfile": {"networkInterfaces": [{"id": NIC, "properties": {"primary": true}}]}
            }
        })
    }

    fn vm(raw: &Value) -> Vm {
        parse_vm(raw).unwrap()
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    #[test]
    fn allows_standalone_vms() {
        assert!(failed(&evaluate(Some(&vm(&raw_vm())))).is_empty());
        assert_eq!(failed(&evaluate(None)), ["exists"]);
    }

    #[test]
    fn refuses_scale_set_and_aks_vms() {
        let mut raw = raw_vm();
        raw["properties"]["virtualMachineScaleSet"] = json!({"id": "/subscriptions/s/resourceGroups/MC_rg/providers/Microsoft.Compute/virtualMachineScaleSets/aks-apps-1"});
        raw["tags"] = json!({"aks-managed-poolName": "apps"});

        assert_eq!(
            failed(&evaluate(Some(&vm(&raw)))),
            ["standalone", "not-aks"]
        );
    }

    #[test]
    fn refuses_unmanaged_os_disks_and_busy_vms() {
        let mut raw = raw_vm();
        raw["properties"]["storageProfile"]["osDisk"]
            .as_object_mut()
            .unwrap()
            .remove("managedDisk");
        raw["properties"]["provisioningState"] = json!("Updating");

        assert_eq!(
            failed(&evaluate(Some(&vm(&raw)))),
            ["managed-os-disk", "idle"]
        );
    }

    #[test]
    fn sets_delete_options_keeping_data_disks() {
        let raw = raw_vm();
        assert!(!delete_options_set(&vm(&raw)));

        let body = delete_options_body(&raw);
        let storage = &body["properties"]["storageProfile"];
        assert_eq!(storage["osDisk"], json!({"deleteOption": "Delete"}));
        assert_eq!(storage["dataDisks"][0]["deleteOption"], "Detach");
        assert_eq!(storage["dataDisks"][0]["lun"], 0);
        assert_eq!(
            storage["dataDisks"][0]["managedDisk"],
            raw["properties"]["storageProfile"]["dataDisks"][0]["managedDisk"]
        );
        let nic = &body["properties"]["networkProfile"]["networkInterfaces"][0];
        assert_eq!(nic["id"], NIC);
        assert_eq!(
            nic["properties"],
            json!({"primary": true, "deleteOption": "Delete"})
        );

        let mut updated = raw.clone();
        updated["properties"]["storageProfile"] = storage.clone();
        updated["properties"]["networkProfile"] = body["properties"]["networkProfile"].clone();
        assert!(delete_options_set(&vm(&updated)));
    }

    #[test]
    fn summarizes_what_is_deleted_and_kept() {
        let s = summary(&vm(&raw_vm()));

        assert_eq!(s.os_disk.as_deref(), Some("vm-1-os"));
        assert_eq!(s.network_interfaces, ["vm-1-nic"]);
        assert_eq!(s.data_disks, ["vm-1-data"]);
    }
}
