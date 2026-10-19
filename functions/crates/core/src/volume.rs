//! Cloud-agnostic decision logic for deleting an orphaned volume.
//!
//! Each cloud reports the volume as a [`Volume`] and its pre-deletion snapshot as a
//! [`SnapshotStatus`]. [`evaluate`] then returns the guards and the next [`Step`]. Keeping the
//! rules here makes them the same on every cloud.
//!
//! A volume is only deleted when it exists, isn't attached, is in a state that allows
//! deletion and was created by Kubernetes for the PersistentVolume the caller names. The
//! function can't see the cluster, so the caller has to confirm the PersistentVolume is gone
//! before asking to delete its volume.
//!
//! With `snapshot` (the default), the volume needs a completed snapshot first. Snapshots can
//! outlast one invocation, so the first execute starts one and is refused. A later execute
//! deletes the volume once the snapshot has completed.
//!
//! A snapshot only counts if the cloud records it as taken of this volume and it was started
//! within [`SNAPSHOT_MAX_AGE_SECS`]. Where the cloud reports when the volume was last
//! detached, the snapshot must also be newer than that, so it can't miss data written since.
//!
//! Skipping the snapshot must be allowed by the installation, see [`snapshot_required`].

use serde::Serialize;

use crate::{Action, Error, Guard};

/// Snapshots started longer ago than this are ignored, and a new one is taken instead.
pub const SNAPSHOT_MAX_AGE_SECS: i64 = 24 * 60 * 60;

/// Tolerated clock difference between the function and the cloud API.
pub const CLOCK_SKEW_SECS: i64 = 5 * 60;

/// Environment variable that allows callers to skip the snapshot with `snapshot: false`.
pub const ALLOW_SKIP_SNAPSHOT_VAR: &str = "ALLOW_SKIP_SNAPSHOT";

/// Whether a snapshot started at `started_at` still counts at `now` (all Unix seconds). It
/// must be recent, not in the future and, if `detached_at` is known, taken after it.
pub fn snapshot_counts(started_at: i64, now: i64, detached_at: Option<i64>) -> bool {
    let age = now - started_at;
    (-CLOCK_SKEW_SECS..=SNAPSHOT_MAX_AGE_SECS).contains(&age)
        && detached_at.is_none_or(|detached| started_at > detached)
}

/// Whether a snapshot is required, given what the caller asked for. Skipping it is rejected
/// unless the installation allows it through [`ALLOW_SKIP_SNAPSHOT_VAR`].
pub fn snapshot_required(requested: bool) -> Result<bool, Error> {
    let allowed = std::env::var(ALLOW_SKIP_SNAPSHOT_VAR).is_ok_and(|v| v == "true");
    check_snapshot_policy(requested, allowed)
}

/// Checks that `pv` is a valid PersistentVolume name (a DNS-1123 subdomain).
pub fn validate_pv_name(pv: &str) -> Result<(), Error> {
    let bytes = pv.as_bytes();
    let alnum = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let valid = (1..=253).contains(&bytes.len())
        && bytes.first().is_some_and(alnum)
        && bytes.last().is_some_and(alnum)
        && bytes.iter().all(|b| alnum(b) || *b == b'-' || *b == b'.');
    if valid {
        Ok(())
    } else {
        Err(Error::invalid_request(format!(
            "pvName {pv:?} is not a PersistentVolume name"
        )))
    }
}

fn check_snapshot_policy(requested: bool, allow_skip: bool) -> Result<bool, Error> {
    if requested || allow_skip {
        Ok(requested)
    } else {
        Err(Error::invalid_request(
            "snapshot: false is not allowed by this installation; the volume is always snapshotted first",
        ))
    }
}

/// Parses an RFC 3339 timestamp, as used by the GCP and Azure APIs, into Unix seconds.
pub fn parse_timestamp(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(time::OffsetDateTime::unix_timestamp)
}

/// The current time in Unix seconds.
pub fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// The volume to delete, as reported by the cloud.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub id: String,
    /// Cloud-specific state, e.g. `available`, `READY` or `Unattached`.
    pub state: String,
    /// Whether the state allows deleting the volume.
    pub deletable: bool,
    /// Whatever the volume is attached to or used by, e.g. instance IDs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attached_to: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_gib: Option<i64>,
    /// The PersistentVolumeClaim the volume was created for, if Kubernetes created it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesClaim>,
    /// When the volume was last detached (Unix seconds), if the cloud reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detached_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesClaim {
    pub pvc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pv: Option<String>,
}

impl KubernetesClaim {
    /// Builds the claim from the standard `kubernetes.io/created-for/*` metadata the CSI
    /// drivers attach to volumes. Returns `None` without a PVC name.
    pub fn from_metadata(
        pvc: Option<&str>,
        namespace: Option<&str>,
        pv: Option<&str>,
    ) -> Option<Self> {
        let pvc = pvc.map(str::trim).filter(|s| !s.is_empty())?;
        let clean = |v: Option<&str>| {
            v.map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        Some(Self {
            pvc: pvc.to_owned(),
            namespace: clean(namespace),
            pv: clean(pv),
        })
    }
}

/// The most recent snapshot this function took of the volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum SnapshotStatus {
    Missing,
    InProgress {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<String>,
    },
    Completed {
        id: String,
    },
    Failed {
        id: String,
    },
}

/// What the function should do after evaluating the guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Change nothing: a plan, or a guard failed.
    Nothing,
    /// Start a snapshot and refuse; the volume is deleted by a later execute.
    CreateSnapshot,
    /// Delete the volume.
    Delete,
}

/// Evaluates the guards for deleting `volume` and decides the next step.
///
/// `pv` is the PersistentVolume the volume must have been created for. `volume` is `None`
/// when the volume doesn't exist. `snapshot` is ignored unless `snapshot_required` is set.
pub fn evaluate(
    action: Action,
    volume: Option<&Volume>,
    pv: &str,
    snapshot_required: bool,
    snapshot: &SnapshotStatus,
) -> (Vec<Guard>, Step) {
    let Some(volume) = volume else {
        return (
            vec![Guard::fail("exists", "volume not found")],
            Step::Nothing,
        );
    };

    let mut guards = vec![
        Guard::pass("exists", format!("found {}", volume.id)),
        attached_guard(volume),
        Guard::check(
            "state",
            volume.deletable,
            format!("state is {}", volume.state),
        ),
        kubernetes_guard(volume, pv),
    ];
    let safe = guards.iter().all(|g| g.passed);

    let (snapshot_guard, step) = match (snapshot_required, snapshot) {
        (false, _) => (Guard::pass("snapshot", "not requested"), Step::Delete),
        (true, SnapshotStatus::Completed { id }) => (
            Guard::pass("snapshot", format!("snapshot {id} completed")),
            Step::Delete,
        ),
        (true, SnapshotStatus::InProgress { id, progress }) => (
            Guard::fail(
                "snapshot",
                format!(
                    "snapshot {id} is in progress{}; execute again once it completes",
                    progress
                        .as_ref()
                        .map(|p| format!(" ({p})"))
                        .unwrap_or_default()
                ),
            ),
            Step::Nothing,
        ),
        (true, SnapshotStatus::Missing | SnapshotStatus::Failed { .. }) => {
            let detail = match (action, snapshot) {
                (Action::Plan, SnapshotStatus::Failed { id }) => format!(
                    "snapshot {id} failed; execute takes a new snapshot first and deletes the volume in a later execute"
                ),
                (Action::Plan, _) => {
                    "execute takes a snapshot first and deletes the volume in a later execute"
                        .to_owned()
                }
                _ => "no completed snapshot yet".to_owned(),
            };
            (
                Guard::check("snapshot", action == Action::Plan, detail),
                Step::CreateSnapshot,
            )
        }
    };
    guards.push(snapshot_guard);

    let step = match action {
        Action::Plan => Step::Nothing,
        Action::Execute if safe => step,
        Action::Execute => Step::Nothing,
    };
    (guards, step)
}

fn attached_guard(volume: &Volume) -> Guard {
    if volume.attached_to.is_empty() {
        Guard::pass("unattached", "not attached")
    } else {
        Guard::fail(
            "unattached",
            format!("attached to {}", volume.attached_to.join(", ")),
        )
    }
}

fn kubernetes_guard(volume: &Volume, pv: &str) -> Guard {
    match &volume.kubernetes {
        Some(claim) if claim.pv.as_deref() != Some(pv) => Guard::fail(
            "kubernetes",
            format!(
                "created for PersistentVolume {}, not {pv}",
                claim.pv.as_deref().unwrap_or("<unknown>")
            ),
        ),
        Some(claim) => Guard::pass(
            "kubernetes",
            format!(
                "created for PersistentVolume {pv} of PVC {}{}",
                claim
                    .namespace
                    .as_ref()
                    .map(|ns| format!("{ns}/"))
                    .unwrap_or_default(),
                claim.pvc
            ),
        ),
        None => Guard::fail(
            "kubernetes",
            "not created by Kubernetes for a PersistentVolumeClaim; only such volumes can be deleted",
        ),
    }
}

/// Default for the `snapshot` parameter, for `#[serde(default = "...")]`.
pub fn default_snapshot() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orphan() -> Volume {
        Volume {
            id: "vol-1".into(),
            state: "available".into(),
            deletable: true,
            attached_to: vec![],
            size_gib: Some(10),
            kubernetes: KubernetesClaim::from_metadata(Some("data"), Some("apps"), Some("pv-1")),
            detached_at: None,
        }
    }

    fn completed() -> SnapshotStatus {
        SnapshotStatus::Completed {
            id: "snap-1".into(),
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
    fn deletes_orphan_after_completed_snapshot() {
        let (guards, step) = evaluate(Action::Execute, Some(&orphan()), "pv-1", true, &completed());

        assert!(failed(&guards).is_empty());
        assert_eq!(step, Step::Delete);
    }

    #[test]
    fn deletes_orphan_without_snapshot_when_not_requested() {
        let (guards, step) = evaluate(
            Action::Execute,
            Some(&orphan()),
            "pv-1",
            false,
            &SnapshotStatus::Missing,
        );

        assert!(failed(&guards).is_empty());
        assert_eq!(step, Step::Delete);
    }

    #[test]
    fn plan_never_changes_anything() {
        for snapshot in [SnapshotStatus::Missing, completed()] {
            let (guards, step) = evaluate(Action::Plan, Some(&orphan()), "pv-1", true, &snapshot);

            assert!(failed(&guards).is_empty());
            assert_eq!(step, Step::Nothing);
        }
    }

    #[test]
    fn execute_takes_snapshot_first() {
        for snapshot in [
            SnapshotStatus::Missing,
            SnapshotStatus::Failed {
                id: "snap-0".into(),
            },
        ] {
            let (guards, step) =
                evaluate(Action::Execute, Some(&orphan()), "pv-1", true, &snapshot);

            assert_eq!(failed(&guards), ["snapshot"]);
            assert_eq!(step, Step::CreateSnapshot);
        }
    }

    #[test]
    fn waits_for_snapshot_in_progress() {
        let snapshot = SnapshotStatus::InProgress {
            id: "snap-1".into(),
            progress: Some("40%".into()),
        };
        let (guards, step) = evaluate(Action::Execute, Some(&orphan()), "pv-1", true, &snapshot);

        assert_eq!(failed(&guards), ["snapshot"]);
        assert!(guards.last().unwrap().detail.contains("40%"));
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn refuses_missing_volume() {
        let (guards, step) = evaluate(
            Action::Execute,
            None,
            "pv-1",
            false,
            &SnapshotStatus::Missing,
        );

        assert_eq!(failed(&guards), ["exists"]);
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn refuses_attached_volume_without_snapshotting() {
        let volume = Volume {
            attached_to: vec!["i-1".into()],
            ..orphan()
        };
        let (guards, step) = evaluate(
            Action::Execute,
            Some(&volume),
            "pv-1",
            true,
            &SnapshotStatus::Missing,
        );

        assert!(failed(&guards).contains(&"unattached"));
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn refuses_volume_in_non_deletable_state() {
        let volume = Volume {
            state: "creating".into(),
            deletable: false,
            ..orphan()
        };
        let (guards, step) = evaluate(Action::Execute, Some(&volume), "pv-1", false, &completed());

        assert_eq!(failed(&guards), ["state"]);
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn refuses_volume_not_created_by_kubernetes() {
        let volume = Volume {
            kubernetes: None,
            ..orphan()
        };
        let (guards, step) = evaluate(Action::Execute, Some(&volume), "pv-1", false, &completed());

        assert_eq!(failed(&guards), ["kubernetes"]);
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn only_recent_snapshots_after_the_last_detach_count() {
        let now = 1_000_000;

        assert!(snapshot_counts(now, now, None));
        assert!(snapshot_counts(now - SNAPSHOT_MAX_AGE_SECS, now, None));
        assert!(!snapshot_counts(now - SNAPSHOT_MAX_AGE_SECS - 1, now, None));
        // Future timestamps only within the clock skew.
        assert!(snapshot_counts(now + CLOCK_SKEW_SECS, now, None));
        assert!(!snapshot_counts(now + CLOCK_SKEW_SECS + 1, now, None));
        // Data written before a later detach isn't in the snapshot.
        assert!(snapshot_counts(now - 10, now, Some(now - 20)));
        assert!(!snapshot_counts(now - 20, now, Some(now - 10)));
        assert!(!snapshot_counts(now - 20, now, Some(now - 20)));
    }

    #[test]
    fn refuses_volume_of_another_persistent_volume() {
        let (guards, step) = evaluate(
            Action::Execute,
            Some(&orphan()),
            "pv-2",
            false,
            &completed(),
        );

        assert_eq!(failed(&guards), ["kubernetes"]);
        assert!(guards[3].detail.contains("pv-1, not pv-2"));
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn refuses_claim_without_persistent_volume_name() {
        let volume = Volume {
            kubernetes: KubernetesClaim::from_metadata(Some("data"), Some("apps"), None),
            ..orphan()
        };
        let (guards, step) = evaluate(Action::Execute, Some(&volume), "pv-1", false, &completed());

        assert_eq!(failed(&guards), ["kubernetes"]);
        assert_eq!(step, Step::Nothing);
    }

    #[test]
    fn validates_persistent_volume_names() {
        for pv in ["pvc-0a1b2c3d-1111-2222-3333-444455556666", "pv-1", "a.b"] {
            assert!(validate_pv_name(pv).is_ok(), "{pv}");
        }
        for pv in ["", "PV-1", "-pv", "pv-", "pv_1", "a/b", &"a".repeat(254)] {
            assert!(validate_pv_name(pv).is_err(), "{pv}");
        }
    }

    #[test]
    fn skipping_the_snapshot_needs_the_installation_to_allow_it() {
        assert!(check_snapshot_policy(true, false).unwrap());
        assert!(check_snapshot_policy(true, true).unwrap());
        assert!(!check_snapshot_policy(false, true).unwrap());
        assert!(matches!(
            check_snapshot_policy(false, false),
            Err(Error::InvalidRequest(_))
        ));
    }

    #[test]
    fn parses_cloud_timestamps() {
        assert_eq!(parse_timestamp("1970-01-01T00:01:00Z"), Some(60));
        assert_eq!(
            parse_timestamp("2026-09-30T10:00:00.123-07:00"),
            Some(1_790_787_600)
        );
        assert_eq!(
            parse_timestamp("2026-09-30T17:00:00.1234567+00:00"),
            Some(1_790_787_600)
        );
        assert_eq!(parse_timestamp("yesterday"), None);
    }

    #[test]
    fn claim_requires_pvc_name() {
        assert_eq!(
            KubernetesClaim::from_metadata(None, Some("apps"), Some("pv")),
            None
        );
        assert_eq!(KubernetesClaim::from_metadata(Some(" "), None, None), None);
        assert_eq!(
            KubernetesClaim::from_metadata(Some("data"), Some(""), None),
            Some(KubernetesClaim {
                pvc: "data".into(),
                namespace: None,
                pv: None
            })
        );
    }
}
