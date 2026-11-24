data "aws_caller_identity" "current" {}

data "aws_partition" "current" {}

# Every Karpenter pool gets its own node role and access entry. The controller's IAM, pod identity and interruption
# queue exist once per cluster, in the stack of the pool that installs Karpenter.
module "karpenter" {
  count   = var.provisioner == "karpenter" ? 1 : 0
  source  = "terraform-aws-modules/eks/aws//modules/karpenter"
  version = "~> 20.37"

  cluster_name          = local.cluster_name
  enable_v1_permissions = true

  create_iam_role                 = var.install_karpenter
  create_pod_identity_association = var.install_karpenter
  enable_spot_termination         = var.install_karpenter

  namespace       = "karpenter"
  service_account = "karpenter"

  queue_name = substr("${local.cluster_name}-karpenter", 0, 80)

  # The module only lets the controller pass its own node role; other devbox pools bring theirs.
  iam_policy_statements = [{
    sid       = "AllowPassingDevboxNodeRoles"
    actions   = ["iam:PassRole"]
    resources = ["arn:${data.aws_partition.current.partition}:iam::${data.aws_caller_identity.current.account_id}:role/${local.node_role_prefix}*"]
    conditions = [{
      test     = "StringEquals"
      variable = "iam:PassedToService"
      values   = ["ec2.amazonaws.com"]
    }]
  }]

  node_iam_role_name            = local.node_role_name
  node_iam_role_use_name_prefix = false
  node_iam_role_additional_policies = {
    AmazonSSMManagedInstanceCore = "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore"
  }
}
