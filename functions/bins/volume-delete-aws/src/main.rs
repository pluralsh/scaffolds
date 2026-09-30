//! Deletes an orphaned EBS volume that Kubernetes created for a PersistentVolumeClaim.
//!
//! The guards and snapshot handling are shared with the other clouds, see
//! [`functions_core::volume`]. The pre-deletion snapshot is tagged with the volume ID so a
//! later execute finds it again.

use aws_sdk_ec2::Client;
use aws_sdk_ec2::error::ProvideErrorMetadata;
use aws_sdk_ec2::types::{
    Filter, ResourceType, Snapshot, SnapshotState, Tag, TagSpecification, VolumeState,
};
use functions_aws::provider_error;
use functions_core::volume::{self, KubernetesClaim, SnapshotStatus, Step, Volume};
use functions_core::{Action, Error, Request, Response};
use serde::{Deserialize, Serialize};

/// Tag on the pre-deletion snapshot holding the ID of the volume it was taken of.
const SNAPSHOT_TAG: &str = "plural.sh/volume-delete";

const PVC_NAME_TAG: &str = "kubernetes.io/created-for/pvc/name";
const PVC_NAMESPACE_TAG: &str = "kubernetes.io/created-for/pvc/namespace";
const PV_NAME_TAG: &str = "kubernetes.io/created-for/pv/name";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    volume_id: String,
    /// PersistentVolume the volume was created for; the caller confirms it no longer exists.
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

fn validate(params: &Params) -> Result<(), Error> {
    let id = params.volume_id.as_str();
    let valid = id.strip_prefix("vol-").is_some_and(|hex| {
        (8..=17).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    });
    if !valid {
        return Err(Error::invalid_request(format!(
            "volumeId {id:?} is not an EBS volume ID (vol-...)"
        )));
    }
    volume::validate_pv_name(&params.pv_name)
}

async fn handle(ec2: &Client, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    validate(&params)?;
    let snapshot_required = volume::snapshot_required(params.snapshot)?;

    let volume = find_volume(ec2, &params.volume_id).await?;
    let snapshot = match (&volume, snapshot_required) {
        (Some(_), true) => latest_snapshot(ec2, &params.volume_id).await?,
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

    match (req.action, step) {
        (Action::Plan, _) => Ok(Response::planned(guards, output)),
        (Action::Execute, Step::Nothing) => Ok(Response::refused(guards).with_result(output)),
        (Action::Execute, Step::CreateSnapshot) => {
            let id = create_snapshot(ec2, &params.volume_id).await?;
            tracing::info!(volume_id = %params.volume_id, snapshot_id = %id, "started pre-deletion snapshot");
            output.snapshot = Some(SnapshotStatus::InProgress { id, progress: None });
            Ok(Response::refused(guards).with_result(output))
        }
        (Action::Execute, Step::Delete) => {
            ec2.delete_volume()
                .volume_id(&params.volume_id)
                .send()
                .await
                .map_err(provider_error)?;
            tracing::info!(volume_id = %params.volume_id, "deleted volume");
            output.deleted = true;
            Ok(Response::done(guards, output))
        }
    }
}

async fn find_volume(ec2: &Client, id: &str) -> Result<Option<Volume>, Error> {
    match ec2.describe_volumes().volume_ids(id).send().await {
        Ok(out) => Ok(out.volumes().first().map(volume_from)),
        Err(err) if err.code() == Some("InvalidVolume.NotFound") => Ok(None),
        Err(err) => Err(provider_error(err)),
    }
}

async fn latest_snapshot(ec2: &Client, volume_id: &str) -> Result<SnapshotStatus, Error> {
    let out = ec2
        .describe_snapshots()
        .owner_ids("self")
        .filters(
            Filter::builder()
                .name(format!("tag:{SNAPSHOT_TAG}"))
                .values(volume_id)
                .build(),
        )
        // EC2 records the source volume itself, so a tag copied onto another snapshot
        // doesn't match.
        .filters(
            Filter::builder()
                .name("volume-id")
                .values(volume_id)
                .build(),
        )
        .send()
        .await
        .map_err(provider_error)?;
    Ok(snapshot_status(out.snapshots(), volume_id, volume::now()))
}

async fn create_snapshot(ec2: &Client, volume_id: &str) -> Result<String, Error> {
    let tags = TagSpecification::builder()
        .resource_type(ResourceType::Snapshot)
        .tags(Tag::builder().key(SNAPSHOT_TAG).value(volume_id).build())
        .tags(
            Tag::builder()
                .key("Name")
                .value(format!("{volume_id} before deletion"))
                .build(),
        )
        .build();
    let out = ec2
        .create_snapshot()
        .volume_id(volume_id)
        .description(format!("Taken by Plural before deleting {volume_id}"))
        .tag_specifications(tags)
        .send()
        .await
        .map_err(provider_error)?;
    out.snapshot_id()
        .map(str::to_owned)
        .ok_or_else(|| Error::provider("CreateSnapshot returned no snapshot ID"))
}

fn volume_from(v: &aws_sdk_ec2::types::Volume) -> Volume {
    let tag = |key: &str| {
        v.tags()
            .iter()
            .find(|t| t.key() == Some(key))
            .and_then(|t| t.value())
    };
    let state = v.state();
    Volume {
        id: v.volume_id().unwrap_or_default().to_owned(),
        state: state.map(|s| s.as_str().to_owned()).unwrap_or_default(),
        deletable: state == Some(&VolumeState::Available),
        attached_to: v
            .attachments()
            .iter()
            .map(|a| a.instance_id().unwrap_or("unknown instance").to_owned())
            .collect(),
        size_gib: v.size().map(i64::from),
        kubernetes: KubernetesClaim::from_metadata(
            tag(PVC_NAME_TAG),
            tag(PVC_NAMESPACE_TAG),
            tag(PV_NAME_TAG),
        ),
        detached_at: None,
    }
}

/// Status of the most recently started snapshot, ignoring snapshots that aren't recent.
fn snapshot_status(snapshots: &[Snapshot], volume_id: &str, now: i64) -> SnapshotStatus {
    // EC2 doesn't report when a volume was detached, so only the age limits a snapshot.
    let Some(latest) = snapshots
        .iter()
        .filter(|s| s.volume_id() == Some(volume_id))
        .filter(|s| {
            s.start_time()
                .is_some_and(|t| volume::snapshot_counts(t.secs(), now, None))
        })
        .max_by_key(|s| s.start_time().copied())
    else {
        return SnapshotStatus::Missing;
    };
    let id = latest.snapshot_id().unwrap_or_default().to_owned();
    match latest.state() {
        Some(SnapshotState::Completed) => SnapshotStatus::Completed { id },
        Some(SnapshotState::Error | SnapshotState::Recoverable) => SnapshotStatus::Failed { id },
        _ => SnapshotStatus::InProgress {
            id,
            progress: latest.progress().map(str::to_owned),
        },
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
    use aws_sdk_ec2::types::VolumeAttachment;
    use aws_smithy_types::DateTime;

    use super::*;

    fn tag(key: &str, value: &str) -> Tag {
        Tag::builder().key(key).value(value).build()
    }

    fn params(id: &str, pv: &str) -> Params {
        Params {
            volume_id: id.into(),
            pv_name: pv.into(),
            snapshot: true,
        }
    }

    fn snapshot(id: &str, volume: &str, secs: i64, state: SnapshotState) -> Snapshot {
        Snapshot::builder()
            .snapshot_id(id)
            .volume_id(volume)
            .start_time(DateTime::from_secs(secs))
            .state(state)
            .progress("40%")
            .build()
    }

    #[test]
    fn validates_volume_ids_and_pv_names() {
        assert!(validate(&params("vol-0123456789abcdef0", "pv-1")).is_ok());
        assert!(validate(&params("vol-12345678", "pvc-1")).is_ok());
        for id in [
            "",
            "vol-",
            "vol-xyz12345",
            "i-0123456789abcdef0",
            "vol-0123456789abcdef01",
        ] {
            assert!(
                validate(&params(id, "pv-1")).is_err(),
                "{id} should be rejected"
            );
        }
        assert!(validate(&params("vol-12345678", "")).is_err());
        assert!(validate(&params("vol-12345678", "PV")).is_err());
    }

    #[test]
    fn maps_orphaned_kubernetes_volume() {
        let v = aws_sdk_ec2::types::Volume::builder()
            .volume_id("vol-1")
            .state(VolumeState::Available)
            .size(20)
            .tags(tag(PVC_NAME_TAG, "data"))
            .tags(tag(PVC_NAMESPACE_TAG, "apps"))
            .tags(tag(PV_NAME_TAG, "pvc-123"))
            .build();

        assert_eq!(
            volume_from(&v),
            Volume {
                id: "vol-1".into(),
                state: "available".into(),
                deletable: true,
                attached_to: vec![],
                size_gib: Some(20),
                kubernetes: KubernetesClaim::from_metadata(
                    Some("data"),
                    Some("apps"),
                    Some("pvc-123")
                ),
                detached_at: None,
            }
        );
    }

    #[test]
    fn maps_attached_volume_without_kubernetes_tags() {
        let v = aws_sdk_ec2::types::Volume::builder()
            .volume_id("vol-1")
            .state(VolumeState::InUse)
            .attachments(VolumeAttachment::builder().instance_id("i-1").build())
            .build();
        let volume = volume_from(&v);

        assert!(!volume.deletable);
        assert_eq!(volume.attached_to, ["i-1"]);
        assert_eq!(volume.kubernetes, None);
    }

    #[test]
    fn picks_latest_recent_snapshot_of_the_volume() {
        let now = 10;

        assert_eq!(snapshot_status(&[], "vol-1", now), SnapshotStatus::Missing);
        assert_eq!(
            snapshot_status(
                &[
                    snapshot("snap-old", "vol-1", 1, SnapshotState::Completed),
                    snapshot("snap-new", "vol-1", 2, SnapshotState::Pending),
                ],
                "vol-1",
                now
            ),
            SnapshotStatus::InProgress {
                id: "snap-new".into(),
                progress: Some("40%".into())
            }
        );
        assert_eq!(
            snapshot_status(
                &[snapshot("snap-1", "vol-1", 1, SnapshotState::Completed)],
                "vol-1",
                now
            ),
            SnapshotStatus::Completed {
                id: "snap-1".into()
            }
        );
        assert_eq!(
            snapshot_status(
                &[snapshot("snap-1", "vol-1", 1, SnapshotState::Error)],
                "vol-1",
                now
            ),
            SnapshotStatus::Failed {
                id: "snap-1".into()
            }
        );
    }

    #[test]
    fn ignores_snapshots_of_other_volumes() {
        // Tagged for vol-1, but EC2 recorded it as taken of vol-2.
        let copied_tag = snapshot("snap-1", "vol-2", 1, SnapshotState::Completed);

        assert_eq!(
            snapshot_status(&[copied_tag], "vol-1", 10),
            SnapshotStatus::Missing
        );
    }

    #[test]
    fn ignores_stale_snapshots() {
        let completed = snapshot("snap-1", "vol-1", 1, SnapshotState::Completed);
        let later = 1 + volume::SNAPSHOT_MAX_AGE_SECS + 1;

        // A snapshot left by an earlier attempt doesn't stand in for data written since.
        assert_eq!(
            snapshot_status(&[completed], "vol-1", later),
            SnapshotStatus::Missing
        );
    }
}
