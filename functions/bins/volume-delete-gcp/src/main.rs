//! Deletes an orphaned zonal persistent disk that Kubernetes created for a
//! PersistentVolumeClaim.
//!
//! The guards and snapshot handling are shared with the other clouds, see
//! [`functions_core::volume`]. The pre-deletion snapshot is labelled with the disk's numeric
//! ID, so it is only ever matched to that disk, not to a later disk with the same name.

use std::collections::HashMap;

use functions_core::volume::{self, KubernetesClaim, SnapshotStatus, Step, Volume};
use functions_core::{Action, Error, Request, Response};
use functions_gcp::compute::{Compute, Disk, Snapshot};
use serde::{Deserialize, Serialize};

/// Label on the pre-deletion snapshot holding the numeric ID of the disk it was taken of.
const SNAPSHOT_LABEL: &str = "plural-sh-volume-delete";

const PVC_NAME_KEY: &str = "kubernetes.io/created-for/pvc/name";
const PVC_NAMESPACE_KEY: &str = "kubernetes.io/created-for/pvc/namespace";
const PV_NAME_KEY: &str = "kubernetes.io/created-for/pv/name";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    zone: String,
    disk: String,
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
    /// Compute Engine operation of the snapshot or deletion that was started.
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
}

fn validate(params: &Params) -> Result<(), Error> {
    if !is_zone(&params.zone) {
        return Err(Error::invalid_request(format!(
            "zone {:?} is not a Compute Engine zone (e.g. us-central1-a)",
            params.zone
        )));
    }
    if !is_resource_name(&params.disk) {
        return Err(Error::invalid_request(format!(
            "disk {:?} is not a Compute Engine disk name",
            params.disk
        )));
    }
    volume::validate_pv_name(&params.pv_name)
}

/// `<region>-<zone letter>`, e.g. `us-central1-a` or `northamerica-northeast1-b`.
fn is_zone(zone: &str) -> bool {
    let Some((region, letter)) = zone.rsplit_once('-') else {
        return false;
    };
    let Some((area, location)) = region.split_once('-') else {
        return false;
    };
    let lower = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase());
    let digits_at = location.find(|c: char| c.is_ascii_digit());
    lower(area)
        && letter.len() == 1
        && lower(letter)
        && digits_at.is_some_and(|i| {
            i > 0 && lower(&location[..i]) && location[i..].bytes().all(|b| b.is_ascii_digit())
        })
}

/// RFC 1035 name as Compute Engine requires: `[a-z]([-a-z0-9]{0,61}[a-z0-9])?`.
fn is_resource_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[bytes.len() - 1] != b'-'
}

async fn handle(client: &reqwest::Client, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    validate(&params)?;
    let snapshot_required = volume::snapshot_required(params.snapshot)?;

    let compute = Compute::connect(client.clone()).await?;
    let disk = compute.disk(&params.zone, &params.disk).await?;
    let volume = disk.as_ref().map(volume_from);
    let snapshot = match (&disk, &volume, snapshot_required) {
        (Some(disk), Some(volume), true) => snapshot_status(
            &compute.snapshots(SNAPSHOT_LABEL, &disk.id).await?,
            &disk.id,
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
        operation: None,
    };

    match (req.action, step, disk) {
        (Action::Plan, ..) => Ok(Response::planned(guards, output)),
        (Action::Execute, Step::CreateSnapshot, Some(disk)) => {
            let name = snapshot_name(&params.disk, volume::now());
            let labels = HashMap::from([(SNAPSHOT_LABEL.to_owned(), disk.id.clone())]);
            let description = format!(
                "Taken by Plural before deleting disk {} in {}",
                params.disk, params.zone
            );
            let op = compute
                .create_snapshot(&params.zone, &params.disk, &name, &description, &labels)
                .await?;
            tracing::info!(zone = %params.zone, disk = %params.disk, snapshot = %name, "started pre-deletion snapshot");
            output.snapshot = Some(SnapshotStatus::InProgress {
                id: name,
                progress: None,
            });
            output.operation = Some(op);
            Ok(Response::refused(guards).with_result(output))
        }
        (Action::Execute, Step::Delete, Some(_)) => {
            let op = compute.delete_disk(&params.zone, &params.disk).await?;
            tracing::info!(zone = %params.zone, disk = %params.disk, operation = %op, "deleting disk");
            output.deleted = true;
            output.operation = Some(op);
            Ok(Response::done(guards, output))
        }
        (Action::Execute, ..) => Ok(Response::refused(guards).with_result(output)),
    }
}

fn volume_from(disk: &Disk) -> Volume {
    let metadata: HashMap<String, String> = disk
        .description
        .as_deref()
        .and_then(|d| serde_json::from_str(d).ok())
        .unwrap_or_default();
    let key = |k: &str| metadata.get(k).map(String::as_str);
    Volume {
        id: disk.name.clone(),
        state: disk.status.clone(),
        deletable: disk.status == "READY",
        attached_to: disk
            .users
            .iter()
            .map(|u| u.rsplit('/').next().unwrap_or(u).to_owned())
            .collect(),
        size_gib: disk.size_gb.as_deref().and_then(|s| s.parse().ok()),
        kubernetes: KubernetesClaim::from_metadata(
            key(PVC_NAME_KEY),
            key(PVC_NAMESPACE_KEY),
            key(PV_NAME_KEY),
        ),
        detached_at: disk
            .last_detach_timestamp
            .as_deref()
            .and_then(volume::parse_timestamp),
    }
}

/// Status of the most recent snapshot taken of the disk `disk_id`, ignoring snapshots that
/// don't count (see [`volume::snapshot_counts`]).
fn snapshot_status(
    snapshots: &[Snapshot],
    disk_id: &str,
    detached_at: Option<i64>,
    now: i64,
) -> SnapshotStatus {
    let Some((_, latest)) = snapshots
        .iter()
        // Compute Engine records the source disk itself, so a label copied onto another
        // snapshot doesn't match.
        .filter(|s| s.source_disk_id.as_deref() == Some(disk_id))
        .filter_map(|s| {
            let created = volume::parse_timestamp(s.creation_timestamp.as_deref()?)?;
            volume::snapshot_counts(created, now, detached_at).then_some((created, s))
        })
        .max_by_key(|(created, _)| *created)
    else {
        return SnapshotStatus::Missing;
    };
    let id = latest.name.clone();
    match latest.status.as_str() {
        "READY" => SnapshotStatus::Completed { id },
        "FAILED" | "DELETING" => SnapshotStatus::Failed { id },
        status => SnapshotStatus::InProgress {
            id,
            progress: Some(status.to_lowercase()),
        },
    }
}

/// A valid, unique snapshot name: `<disk, shortened>-predelete-<unix time>`.
fn snapshot_name(disk: &str, now: i64) -> String {
    let base: String = disk.chars().take(40).collect();
    format!("{}-predelete-{now}", base.trim_end_matches('-'))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = functions_http::https_client().map_err(std::io::Error::other)?;

    functions_http::run(move |req| {
        let client = client.clone();
        async move { handle(&client, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(description: Option<&str>, users: &[&str]) -> Disk {
        Disk {
            id: "4242".into(),
            name: "pvc-1".into(),
            status: "READY".into(),
            size_gb: Some("10".into()),
            users: users.iter().map(|u| (*u).to_owned()).collect(),
            description: description.map(str::to_owned),
            labels: HashMap::new(),
            last_detach_timestamp: None,
        }
    }

    fn snapshot(name: &str, created: &str, status: &str) -> Snapshot {
        Snapshot {
            name: name.into(),
            status: status.into(),
            creation_timestamp: Some(created.into()),
            source_disk_id: Some("4242".into()),
        }
    }

    #[test]
    fn validates_zones_and_disk_names() {
        let params = |zone: &str, disk: &str, pv: &str| Params {
            zone: zone.into(),
            disk: disk.into(),
            pv_name: pv.into(),
            snapshot: true,
        };
        assert!(validate(&params("us-central1-a", "pvc-1", "pvc-1")).is_ok());
        assert!(validate(&params("us-central1-a", "pvc-1", "")).is_err());

        for zone in [
            "us-central1-a",
            "europe-west4-b",
            "northamerica-northeast1-c",
            "me-central2-a",
        ] {
            assert!(is_zone(zone), "{zone}");
        }
        for zone in [
            "",
            "us-central1",
            "us-central1-ab",
            "US-central1-a",
            "us-central-a",
            "us-1-a",
            "us-central1-a/../x",
        ] {
            assert!(!is_zone(zone), "{zone}");
        }
        assert!(is_resource_name("pvc-0a1b2c3d"));
        for name in [
            "",
            "Disk",
            "1disk",
            "disk-",
            "disk_1",
            "a/b",
            &"a".repeat(64),
        ] {
            assert!(!is_resource_name(name), "{name}");
        }
    }

    #[test]
    fn maps_csi_disk_metadata() {
        let description = r#"{"kubernetes.io/created-for/pv/name":"pvc-123","kubernetes.io/created-for/pvc/name":"data","kubernetes.io/created-for/pvc/namespace":"apps","storage.gke.io/created-by":"pd.csi.storage.gke.io"}"#;
        let volume = volume_from(&disk(Some(description), &[]));

        assert!(volume.deletable && volume.attached_to.is_empty());
        assert_eq!(volume.size_gib, Some(10));
        assert_eq!(
            volume.kubernetes,
            KubernetesClaim::from_metadata(Some("data"), Some("apps"), Some("pvc-123"))
        );
    }

    #[test]
    fn maps_attached_disk_without_kubernetes_metadata() {
        let volume = volume_from(&disk(
            Some("hand-made disk"),
            &["https://www.googleapis.com/compute/v1/projects/p/zones/z/instances/node-1"],
        ));

        assert_eq!(volume.attached_to, ["node-1"]);
        assert_eq!(volume.kubernetes, None);
    }

    #[test]
    fn picks_latest_recent_snapshot() {
        let now = volume::parse_timestamp("2026-09-30T12:00:00Z").unwrap();

        assert_eq!(
            snapshot_status(&[], "4242", None, now),
            SnapshotStatus::Missing
        );
        assert_eq!(
            snapshot_status(
                &[
                    snapshot("old", "2026-09-30T03:00:00.000-07:00", "READY"),
                    snapshot("new", "2026-09-30T11:00:00Z", "UPLOADING"),
                ],
                "4242",
                None,
                now
            ),
            SnapshotStatus::InProgress {
                id: "new".into(),
                progress: Some("uploading".into())
            }
        );
        assert_eq!(
            snapshot_status(
                &[snapshot("s", "2026-09-30T11:00:00Z", "READY")],
                "4242",
                None,
                now
            ),
            SnapshotStatus::Completed { id: "s".into() }
        );
        assert_eq!(
            snapshot_status(
                &[snapshot("s", "2026-09-30T11:00:00Z", "FAILED")],
                "4242",
                None,
                now
            ),
            SnapshotStatus::Failed { id: "s".into() }
        );
        // A snapshot left by an earlier attempt doesn't stand in for data written since.
        assert_eq!(
            snapshot_status(
                &[snapshot("s", "2026-09-28T11:00:00Z", "READY")],
                "4242",
                None,
                now
            ),
            SnapshotStatus::Missing
        );
    }

    #[test]
    fn only_counts_snapshots_of_this_disk_after_its_last_detach() {
        let now = volume::parse_timestamp("2026-09-30T12:00:00Z").unwrap();
        let other_disk = Snapshot {
            source_disk_id: Some("9999".into()),
            ..snapshot("copy", "2026-09-30T11:00:00Z", "READY")
        };
        let before_detach = snapshot("s", "2026-09-30T10:00:00Z", "READY");
        let detached_at = volume::parse_timestamp("2026-09-30T10:30:00Z");

        // Labelled for this disk, but Compute Engine recorded another source disk.
        assert_eq!(
            snapshot_status(&[other_disk], "4242", None, now),
            SnapshotStatus::Missing
        );
        // Written to again after the snapshot: the snapshot misses that data.
        assert_eq!(
            snapshot_status(
                std::slice::from_ref(&before_detach),
                "4242",
                detached_at,
                now
            ),
            SnapshotStatus::Missing
        );
        assert_eq!(
            snapshot_status(&[before_detach], "4242", None, now),
            SnapshotStatus::Completed { id: "s".into() }
        );
    }

    #[test]
    fn reads_last_detach_time() {
        let volume = volume_from(&Disk {
            last_detach_timestamp: Some("2026-09-30T03:00:00.000-07:00".into()),
            ..disk(None, &[])
        });

        assert_eq!(
            volume.detached_at,
            volume::parse_timestamp("2026-09-30T10:00:00Z")
        );
    }

    #[test]
    fn snapshot_names_are_valid() {
        let name = snapshot_name(&"a".repeat(63), 1_790_787_600);

        assert!(is_resource_name(&name), "{name}");
        assert!(is_resource_name(&snapshot_name("pvc-1", 1_790_787_600)));
        assert!(is_resource_name(&snapshot_name(
            &format!("{}-b", "a".repeat(39)),
            1
        )));
    }
}
