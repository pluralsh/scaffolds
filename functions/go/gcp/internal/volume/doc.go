// Package volume holds the cloud-agnostic logic that decides whether to delete an orphaned
// volume.
//
// Each cloud describes the volume and its pre-deletion snapshot with [Volume] and
// [SnapshotStatus]. [Deletion.Evaluate] then returns the guards and the single [Step] to
// take. Keeping the rules here makes them the same on every cloud.
//
// A volume is only deleted if it exists, isn't attached, is in a deletable state, and was
// created by Kubernetes for the PersistentVolume the caller names. The function can't see the
// cluster, so the caller must confirm the PersistentVolume is gone.
//
// By default a completed snapshot is required first. Snapshots can outlast an invocation, so
// the first execute starts one and is refused. A later execute deletes the volume once the
// snapshot completes. A snapshot only counts if the cloud records it as taken of this volume,
// it started within [SnapshotMaxAgeSecs], and it started after the volume was last detached
// (where the cloud reports that). This stops an older snapshot from standing in for data
// written since, see [SnapshotWindow]. Skipping the snapshot must be allowed by the
// installation, see [SnapshotPolicy].
package volume
