# Devbox homes are seeded and moved between zones through VolumeSnapshots. GKE serves them, with its own
# snapshot controller, whenever the Compute Engine persistent disk CSI driver is on, as it is by default.
data "google_container_cluster" "cluster" {
  name     = local.cluster_context.cluster_name
  project  = local.cluster_context.project_id
  location = local.cluster_context.region
}

check "volume_snapshots" {
  assert {
    condition     = !var.install_substrate || try(data.google_container_cluster.cluster.addons_config[0].gce_persistent_disk_csi_driver_config[0].enabled, true)
    error_message = "The cluster's Compute Engine persistent disk CSI driver is off, so it serves no VolumeSnapshots and can't run Block devbox homes; enable the GcePersistentDiskCsiDriver add-on."
  }
}
