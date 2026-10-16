//! Deletes an orphaned managed disk that Kubernetes created for a PersistentVolumeClaim.
//!
//! Guards and snapshot handling are shared with the other clouds in [`functions_core::volume`].
//! The pre-deletion snapshot is an incremental snapshot in the disk's resource group, tagged
//! with the disk's resource ID so a later execute finds it.

use std::collections::HashMap;

use functions_azure::arm::{self, Connector, Disk, ResourceId, Snapshot};
use functions_core::volume::{self, KubernetesClaim, SnapshotStatus, Step, Volume};
use functions_core::{Action, Error, Request, Response};
use serde::{Deserialize, Serialize};

/// Tag on the pre-deletion snapshot holding the resource ID of the disk it was taken of.
/// Azure tag names can't contain `/`.
const SNAPSHOT_TAG: &str = "plural.sh-volume-delete";

/// Tags the Azure Disk CSI driver sets, with `/` replaced by `-`.
const PVC_NAME_TAG: &str = "kubernetes.io-created-for-pvc-name";
const PVC_NAMESPACE_TAG: &str = "kubernetes.io-created-for-pvc-namespace";
const PV_NAME_TAG: &str = "kubernetes.io-created-for-pv-name";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    disk_id: String,
    /// PersistentVolume the disk was created for; the caller confirms it no longer exists.
    pv_name: String,
    /// Take a snapshot and wait for it to complete before deleting.
    #[serde(default = "volume::default_snapshot")]
    snapshot: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<Volume>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<SnapshotStatus>,
    deleted: bool,
}

async fn handle(connector: &Connector, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    let id = ResourceId::parse(&params.disk_id, &[arm::DISK]).ok_or_else(|| {
        Error::invalid_request(format!(
            "diskId {:?} is not a managed disk resource ID (/subscriptions/<id>/resourceGroups/<group>/providers/Microsoft.Compute/disks/<name>)",
            params.disk_id
        ))
    })?;
    volume::validate_pv_name(&params.pv_name)?;
    let snapshot_required = volume::snapshot_required(params.snapshot)?;

    let arm = connector.connect().await?;
    let disk = arm.disk(&id).await?;
    let volume = disk.as_ref().map(volume_from);
    let snapshot = match (&disk, &volume, snapshot_required) {
        (Some(disk), Some(volume), true) => snapshot_status(
            &arm.snapshots(&id).await?,
            disk,
            volume.detached_at,
            volume::now(),
        ),
        _ => SnapshotStatus::Missing,
    };
    let (guards, step) = volume::evaluate(
        req.action,
        volume.as_ref(),
        &params.pv_name,
        snapshot_required,
        &snapshot,
    );
    let mut output = Output {
        volume,
        snapshot: snapshot_required.then_some(snapshot),
        deleted: false,
    };

    match (req.action, step, disk) {
        (Action::Plan, ..) => Ok(Response::planned(guards, output)),
        (Action::Execute, Step::CreateSnapshot, Some(disk)) => {
            let name = snapshot_name(&id.name, volume::now());
            let tags = HashMap::from([(SNAPSHOT_TAG.to_owned(), id.id().to_lowercase())]);
            arm.create_snapshot(&id, &disk.location, &name, &tags)
                .await?;
            tracing::info!(disk = %id.id(), snapshot = %name, "started pre-deletion snapshot");
            output.snapshot = Some(SnapshotStatus::InProgress {
                id: name,
                progress: None,
            });
            Ok(Response::refused(guards).with_result(output))
        }
        (Action::Execute, Step::Delete, Some(_)) => {
            arm.delete_disk(&id).await?;
            tracing::info!(disk = %id.id(), "deleting disk");
            output.deleted = true;
            Ok(Response::done(guards, output))
        }
        (Action::Execute, ..) => Ok(Response::refused(guards).with_result(output)),
    }
}

fn volume_from(disk: &Disk) -> Volume {
    let tag = |key: &str| disk.tags.get(key).map(String::as_str);
    let props = &disk.properties;
    Volume {
        id: disk.id.clone(),
        state: format!("{}/{}", props.disk_state, props.provisioning_state),
        deletable: props.disk_state == "Unattached" && props.provisioning_state == "Succeeded",
        attached_to: disk
            .managed_by
            .iter()
            .chain(&disk.managed_by_extended)
            .filter(|v| !v.is_empty())
            .map(|v| v.rsplit('/').next().unwrap_or(v).to_owned())
            .collect(),
        size_gib: props.disk_size_gb,
        kubernetes: KubernetesClaim::from_metadata(
            tag(PVC_NAME_TAG),
            tag(PVC_NAMESPACE_TAG),
            tag(PV_NAME_TAG),
        ),
        detached_at: props
            .last_ownership_update_time
            .as_deref()
            .and_then(volume::parse_timestamp),
    }
}

/// Disk types whose snapshots keep copying data in the background after provisioning.
fn copies_in_background(disk: &Disk) -> bool {
    disk.sku
        .as_ref()
        .is_some_and(|sku| matches!(sku.name.as_str(), "PremiumV2_LRS" | "UltraSSD_LRS"))
}

/// Status of the most recent snapshot taken of `disk`, ignoring snapshots that don't count
/// (see [`volume::snapshot_counts`]).
fn snapshot_status(
    snapshots: &[Snapshot],
    disk: &Disk,
    detached_at: Option<i64>,
    now: i64,
) -> SnapshotStatus {
    let disk_id = disk.id.to_lowercase();
    let Some((_, latest)) = snapshots
        .iter()
        .filter(|s| {
            s.tags
                .get(SNAPSHOT_TAG)
                .is_some_and(|v| v.to_lowercase() == disk_id)
        })
        .filter(|s| taken_of(s, disk))
        .filter_map(|s| {
            let created = volume::parse_timestamp(s.properties.time_created.as_deref()?)?;
            volume::snapshot_counts(created, now, detached_at).then_some((created, s))
        })
        .max_by_key(|(created, _)| *created)
    else {
        return SnapshotStatus::Missing;
    };
    let id = latest.name.clone();
    let props = &latest.properties;
    match (props.provisioning_state.as_str(), props.completion_percent) {
        ("Failed" | "Canceled", _) => SnapshotStatus::Failed { id },
        // Snapshots of disks that copy in the background report their progress; the others
        // are complete once provisioned.
        ("Succeeded", None) if !copies_in_background(disk) => SnapshotStatus::Completed { id },
        ("Succeeded", Some(p)) if p >= 100.0 => SnapshotStatus::Completed { id },
        (state, percent) => SnapshotStatus::InProgress {
            id,
            progress: Some(match percent {
                Some(p) => format!("{p}%"),
                None => state.to_lowercase(),
            }),
        },
    }
}

/// Whether Azure recorded `snapshot` as taken of `disk`. Prefers `uniqueId` over the resource
/// ID, since it changes when a disk is re-created under the same name.
fn taken_of(snapshot: &Snapshot, disk: &Disk) -> bool {
    let Some(source) = &snapshot.properties.creation_data else {
        return false;
    };
    match (&disk.properties.unique_id, &source.source_unique_id) {
        (Some(disk_uid), Some(source_uid)) => disk_uid.eq_ignore_ascii_case(source_uid),
        (Some(_), None) => false,
        (None, _) => source
            .source_resource_id
            .as_deref()
            .is_some_and(|src| src.eq_ignore_ascii_case(&disk.id)),
    }
}

/// A valid, unique snapshot name: `<disk, shortened>-predelete-<unix time>`, at most 80 chars.
fn snapshot_name(disk: &str, now: i64) -> String {
    let base: String = disk.chars().take(50).collect();
    format!("{}-predelete-{now}", base.trim_end_matches(['-', '.']))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = functions_http::https_client().map_err(std::io::Error::other)?;
    let connector = Connector::from_env(client).map_err(std::io::Error::other)?;

    functions_http::run(move |req| {
        let connector = connector.clone();
        async move { handle(&connector, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use functions_azure::arm::{CreationData, DiskProperties, Sku, SnapshotProperties};

    use super::*;

    const DISK: &str = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/MC_rg/providers/Microsoft.Compute/disks/pvc-1";

    fn disk(state: &str, managed_by: Option<&str>, tags: &[(&str, &str)]) -> Disk {
        Disk {
            id: DISK.into(),
            name: "pvc-1".into(),
            location: "eastus".into(),
            tags: tags
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            managed_by: managed_by.map(str::to_owned),
            managed_by_extended: vec![],
            sku: Some(Sku {
                name: "Premium_LRS".into(),
            }),
            properties: DiskProperties {
                disk_state: state.into(),
                provisioning_state: "Succeeded".into(),
                disk_size_gb: Some(32),
                unique_id: Some("uid-1".into()),
                last_ownership_update_time: None,
            },
        }
    }

    fn snapshot(name: &str, created: &str, state: &str, percent: Option<f64>) -> Snapshot {
        Snapshot {
            name: name.into(),
            tags: HashMap::from([(SNAPSHOT_TAG.to_owned(), DISK.to_lowercase())]),
            properties: SnapshotProperties {
                provisioning_state: state.into(),
                completion_percent: percent,
                time_created: Some(created.into()),
                creation_data: Some(CreationData {
                    source_resource_id: Some(DISK.into()),
                    source_unique_id: Some("uid-1".into()),
                }),
            },
        }
    }

    fn now() -> i64 {
        volume::parse_timestamp("2026-09-30T12:00:00Z").unwrap()
    }

    #[test]
    fn maps_orphaned_csi_disk() {
        let volume = volume_from(&disk(
            "Unattached",
            None,
            &[
                (PVC_NAME_TAG, "data"),
                (PVC_NAMESPACE_TAG, "apps"),
                (PV_NAME_TAG, "pvc-1"),
            ],
        ));

        assert!(volume.deletable && volume.attached_to.is_empty());
        assert_eq!(
            volume.kubernetes,
            KubernetesClaim::from_metadata(Some("data"), Some("apps"), Some("pvc-1"))
        );
    }

    #[test]
    fn maps_attached_and_reserved_disks() {
        let attached = volume_from(&disk(
            "Attached",
            Some(
                "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm-1",
            ),
            &[],
        ));
        let reserved = volume_from(&disk("Reserved", None, &[]));

        assert!(!attached.deletable);
        assert_eq!(attached.attached_to, ["vm-1"]);
        assert!(
            !reserved.deletable,
            "a reserved disk belongs to a stopped VM"
        );
        assert_eq!(reserved.kubernetes, None);
    }

    #[test]
    fn picks_latest_recent_snapshot_of_this_disk() {
        let d = disk("Unattached", None, &[]);
        let other = Snapshot {
            tags: HashMap::from([(
                SNAPSHOT_TAG.to_owned(),
                "/subscriptions/x/other-disk".into(),
            )]),
            ..snapshot("other", "2026-09-30T11:30:00Z", "Succeeded", None)
        };

        assert_eq!(
            snapshot_status(&[], &d, None, now()),
            SnapshotStatus::Missing
        );
        assert_eq!(
            snapshot_status(
                &[
                    snapshot("old", "2026-09-30T09:00:00+00:00", "Succeeded", Some(100.0)),
                    snapshot(
                        "new",
                        "2026-09-30T11:00:00.1234567+00:00",
                        "Succeeded",
                        Some(42.5)
                    ),
                    other,
                ],
                &d,
                None,
                now()
            ),
            SnapshotStatus::InProgress {
                id: "new".into(),
                progress: Some("42.5%".into())
            }
        );
        assert_eq!(
            snapshot_status(
                &[snapshot(
                    "s",
                    "2026-09-30T11:00:00Z",
                    "Succeeded",
                    Some(100.0)
                )],
                &d,
                None,
                now()
            ),
            SnapshotStatus::Completed { id: "s".into() }
        );
        assert_eq!(
            snapshot_status(
                &[snapshot("s", "2026-09-30T11:00:00Z", "Failed", None)],
                &d,
                None,
                now()
            ),
            SnapshotStatus::Failed { id: "s".into() }
        );
        // A snapshot left by an earlier attempt doesn't stand in for data written since.
        assert_eq!(
            snapshot_status(
                &[snapshot("s", "2026-09-28T11:00:00Z", "Succeeded", None)],
                &d,
                None,
                now()
            ),
            SnapshotStatus::Missing
        );
    }

    #[test]
    fn ignores_snapshots_of_a_disk_recreated_under_the_same_name() {
        let recreated = Disk {
            properties: DiskProperties {
                unique_id: Some("uid-2".into()),
                ..disk("Unattached", None, &[]).properties
            },
            ..disk("Unattached", None, &[])
        };
        let old = snapshot("s", "2026-09-30T11:00:00Z", "Succeeded", Some(100.0));

        assert_eq!(
            snapshot_status(&[old], &recreated, None, now()),
            SnapshotStatus::Missing
        );
    }

    #[test]
    fn ignores_snapshots_taken_before_the_last_detach() {
        let d = disk("Unattached", None, &[]);
        let taken = snapshot("s", "2026-09-30T10:00:00Z", "Succeeded", Some(100.0));
        let detached = volume::parse_timestamp("2026-09-30T10:30:00Z");

        assert_eq!(
            snapshot_status(std::slice::from_ref(&taken), &d, detached, now()),
            SnapshotStatus::Missing
        );
        assert_eq!(
            snapshot_status(&[taken], &d, None, now()),
            SnapshotStatus::Completed { id: "s".into() }
        );
    }

    #[test]
    fn waits_for_background_copy_of_premium_v2_and_ultra_disks() {
        let v2 = Disk {
            sku: Some(Sku {
                name: "PremiumV2_LRS".into(),
            }),
            ..disk("Unattached", None, &[])
        };
        let no_progress = snapshot("s", "2026-09-30T11:00:00Z", "Succeeded", None);

        assert!(matches!(
            snapshot_status(std::slice::from_ref(&no_progress), &v2, None, now()),
            SnapshotStatus::InProgress { .. }
        ));
        assert_eq!(
            snapshot_status(&[no_progress], &disk("Unattached", None, &[]), None, now()),
            SnapshotStatus::Completed { id: "s".into() }
        );
    }

    #[test]
    fn reads_last_detach_time() {
        let mut d = disk("Unattached", None, &[]);
        d.properties.last_ownership_update_time = Some("2026-09-30T10:00:00.1234567+00:00".into());

        assert_eq!(
            volume_from(&d).detached_at,
            volume::parse_timestamp("2026-09-30T10:00:00Z")
        );
    }

    #[test]
    fn snapshot_names_are_valid_and_short() {
        for disk in ["pvc-1", &"a".repeat(80), &format!("{}-.x", "a".repeat(48))] {
            let name = snapshot_name(disk, 1_790_787_600);

            assert!(name.len() <= 80, "{name}");
            assert!(!name.contains("-.") && !name.contains(".-"), "{name}");
        }
    }
}

#[cfg(test)]
mod handler_tests {
    use functions_azure::mock::{Method, MockArm};
    use serde_json::{Value, json};

    use super::*;

    const RG: &str = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/MC_rg";

    fn disk_path() -> String {
        format!("{RG}/providers/Microsoft.Compute/disks/pvc-1")
    }

    fn snapshots_path() -> String {
        format!("{RG}/providers/Microsoft.Compute/snapshots")
    }

    fn disk(state: &str) -> Value {
        json!({
            "id": disk_path(), "name": "pvc-1", "location": "eastus",
            "tags": {PV_NAME_TAG: "pvc-1", PVC_NAME_TAG: "data", PVC_NAMESPACE_TAG: "apps"},
            "sku": {"name": "Premium_LRS"},
            "properties": {"diskState": state, "provisioningState": "Succeeded", "diskSizeGB": 32, "uniqueId": "uid-1"}
        })
    }

    fn completed_snapshot() -> Value {
        let created = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
        json!({
            "name": "pvc-1-predelete",
            "tags": {SNAPSHOT_TAG: disk_path().to_lowercase()},
            "properties": {
                "provisioningState": "Succeeded",
                "timeCreated": created.format(&time::format_description::well_known::Rfc3339).unwrap(),
                "creationData": {"sourceResourceId": disk_path(), "sourceUniqueId": "uid-1"}
            }
        })
    }

    async fn call(mock: &MockArm, action: &str, pv: &str) -> Value {
        let req =
            serde_json::from_value(json!({"action": action, "diskId": disk_path(), "pvName": pv}))
                .unwrap();
        serde_json::to_value(handle(&mock.connector(), req).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn plan_reads_only() {
        let mock = MockArm::start().await;
        mock.on(Method::GET, &disk_path(), 200, disk("Unattached"))
            .on(Method::GET, &snapshots_path(), 200, json!({"value": []}));

        assert_eq!(call(&mock, "plan", "pvc-1").await["outcome"], "planned");
        assert!(mock.writes().is_empty());
    }

    #[tokio::test]
    async fn first_execute_only_snapshots() {
        let mock = MockArm::start().await;
        mock.on(Method::GET, &disk_path(), 200, disk("Unattached"))
            .on(Method::GET, &snapshots_path(), 200, json!({"value": []}))
            .on_any(Method::PUT, 201, json!({}));

        let resp = call(&mock, "execute", "pvc-1").await;

        assert_eq!(resp["outcome"], "refused");
        let writes = mock.writes();
        assert_eq!(writes.len(), 1, "no delete before the snapshot completes");
        assert_eq!(writes[0].method, Method::PUT);
        assert!(
            writes[0]
                .path
                .starts_with(&format!("{}/pvc-1-predelete-", snapshots_path()))
        );
        assert_eq!(
            writes[0].body["properties"]["creationData"]["sourceResourceId"],
            disk_path()
        );
        assert_eq!(
            writes[0].body["tags"][SNAPSHOT_TAG],
            disk_path().to_lowercase()
        );
    }

    #[tokio::test]
    async fn deletes_after_a_completed_snapshot() {
        let mock = MockArm::start().await;
        mock.on(Method::GET, &disk_path(), 200, disk("Unattached"))
            .on(
                Method::GET,
                &snapshots_path(),
                200,
                json!({"value": [completed_snapshot()]}),
            )
            .on(Method::DELETE, &disk_path(), 202, Value::Null);

        let resp = call(&mock, "execute", "pvc-1").await;

        assert_eq!(resp["outcome"], "done", "{resp}");
        let writes = mock.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            (writes[0].method.clone(), writes[0].path.clone()),
            (Method::DELETE, disk_path())
        );
    }

    #[tokio::test]
    async fn never_touches_attached_or_other_disks() {
        for (state, pv) in [("Attached", "pvc-1"), ("Unattached", "pvc-2")] {
            let mock = MockArm::start().await;
            mock.on(Method::GET, &disk_path(), 200, disk(state)).on(
                Method::GET,
                &snapshots_path(),
                200,
                json!({"value": [completed_snapshot()]}),
            );

            assert_eq!(
                call(&mock, "execute", pv).await["outcome"],
                "refused",
                "{state} {pv}"
            );
            assert!(mock.writes().is_empty(), "{state} {pv}");
        }
    }
}
