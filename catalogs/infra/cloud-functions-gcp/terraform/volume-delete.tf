# volume-delete: deletes an orphaned zonal persistent disk that Kubernetes created for a
# PersistentVolumeClaim, after snapshotting it. See functions/go/gcp/docs/volume-delete.md.

variable "allow_skip_snapshot" {
  type        = bool
  description = "Let callers skip the snapshot taken before a disk is deleted. When false, volume-delete always snapshots first and its tool schema has no snapshot input."
  default     = false
}

locals {
  # Without allow_skip_snapshot, the `snapshot` input is dropped from the tool schema and the
  # function also rejects `snapshot: false`.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  volume_delete = {
    entry_point = "VolumeDelete"
    description = "Deletes an unattached zonal persistent disk that Kubernetes created for a PersistentVolume. Before calling it, confirm in the cluster that the PersistentVolume no longer exists and pass its name as pvName. Use action plan first; execute takes a snapshot and keeps the disk, and a later execute deletes it once the snapshot has completed."
    memory      = "512Mi"
    timeout     = 30
    destructive = true
    environment = {
      ALLOW_SKIP_SNAPSHOT = tostring(var.allow_skip_snapshot)
    }
    # Every disk in the project; GCP IAM can't restrict it to Kubernetes disks, so the function
    # does those checks itself.
    permissions = [
      "compute.disks.get",
      "compute.disks.delete",
      "compute.disks.createSnapshot",
      "compute.snapshots.create",
      "compute.snapshots.list",
      "compute.snapshots.setLabels",
    ]
    schema = jsonencode(merge(local.volume_delete_schema, { properties = local.volume_delete_properties }))
  }
}
