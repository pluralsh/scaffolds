# Devbox homes are seeded and moved between zones through VolumeSnapshots. AKS serves them with the Azure Disk
# CSI driver and its snapshot controller, both on by default in the cluster's storage profile.
check "volume_snapshots" {
  assert {
    condition = !var.install_substrate || alltrue([
      try(data.azurerm_kubernetes_cluster.cluster.storage_profile[0].disk_driver_enabled, true),
      try(data.azurerm_kubernetes_cluster.cluster.storage_profile[0].snapshot_controller_enabled, true),
    ])
    error_message = "The cluster's Azure Disk CSI driver or snapshot controller is off, so it can't run Block devbox homes; enable both in its storage profile (az aks update --enable-disk-driver --enable-snapshot-controller)."
  }
}
