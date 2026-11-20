module "karpenter" {
  count   = var.provisioner == "karpenter" ? 1 : 0
  source  = "terraform-aws-modules/eks/aws//modules/karpenter"
  version = "~> 20.37"

  cluster_name          = local.cluster_name
  enable_v1_permissions = true

  namespace                       = "karpenter"
  service_account                 = "karpenter"
  create_pod_identity_association = true

  queue_name = substr("${local.cluster_name}-karpenter", 0, 80)

  node_iam_role_name            = local.node_role_name
  node_iam_role_use_name_prefix = false
  node_iam_role_additional_policies = {
    AmazonSSMManagedInstanceCore = "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore"
  }
}
