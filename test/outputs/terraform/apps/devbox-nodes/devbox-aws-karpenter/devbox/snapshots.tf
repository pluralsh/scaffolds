# Devbox homes are seeded and moved between zones through VolumeSnapshots, which EKS only serves once the
# snapshot controller and its CRDs are installed. The EBS CSI driver already runs the snapshotter sidecar.
resource "aws_eks_addon" "snapshot_controller" {
  count        = var.install_snapshot_controller ? 1 : 0
  cluster_name = local.cluster_name
  addon_name   = "snapshot-controller"
}

# The disks themselves come from the EBS CSI driver, which this only warns about: it may run outside EKS add-ons.
check "ebs_csi_driver" {
  data "aws_eks_addon" "ebs_csi" {
    cluster_name = local.cluster_name
    addon_name   = "aws-ebs-csi-driver"
  }

  assert {
    condition     = !var.install_substrate || data.aws_eks_addon.ebs_csi.addon_version != ""
    error_message = "The cluster has no aws-ebs-csi-driver add-on, which devbox homes get their disks from; install it (with an IAM role allowed to create and snapshot volumes) unless the driver runs another way."
  }
}
