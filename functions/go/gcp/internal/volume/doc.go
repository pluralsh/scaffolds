// Package volume holds the cloud-agnostic decision logic for deleting an orphaned volume.
//
// Each cloud describes the volume and its pre-deletion snapshot with [Volume] and
// [SnapshotStatus], and [Deletion.Evaluate] decides the guards and the single [Step] to
// take. Keeping the rules here makes them identical on every cloud.
//
// A volume is only deleted when it exists, isn't attached, is in a state that allows
// deletion and was created by Kubernetes for the PersistentVolume the caller names. The
// function can't see the cluster, so the caller has to confirm the PersistentVolume is gone
// before asking to delete its volume.
//
// With a snapshot (the default), a completed snapshot of the volume is required first:
// snapshots can take longer than a single invocation, so the first execute starts one and is
// refused, and a later execute deletes the volume once the snapshot has completed. A snapshot
// only counts if the cloud records it as taken of this volume, it was started within
// [SnapshotMaxAgeSecs] and, where the cloud reports it, after the volume was last detached,
// so an older snapshot can't stand in for data written to the volume since, see
// [SnapshotWindow]. Skipping the snapshot has to be allowed by the installation, see
// [SnapshotPolicy].
package volume
