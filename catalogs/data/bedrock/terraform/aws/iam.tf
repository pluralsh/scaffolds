locals {
  cluster_name = jsondecode(data.plural_service_context.mgmt.configuration)["cluster_name"]
}

resource "aws_iam_policy" "bedrock" {
  name        = "${local.cluster_name}-bedrock-policy"
  description = "Policy granting only Bedrock permissions"
  policy      = data.aws_iam_policy_document.bedrock.json
}

resource "aws_iam_role" "bedrock" {
  name               = "${local.cluster_name}-bedrock-role"
  assume_role_policy = data.aws_iam_policy_document.bedrock_assume_role_policy.json
}

data "aws_iam_policy_document" "bedrock_assume_role_policy" {
  statement {
    actions   = ["sts:AssumeRoleWithWebIdentity"]
    effect    = "Allow"

    principals {
      type        = "Federated"
      identifiers = [
        "arn:aws:iam::${data.aws_caller_identity.current.account_id}:oidc-provider/${replace(data.aws_eks_cluster.eks_cluster.identity[0].oidc[0].issuer, "https://", "")}"
      ]
    }

    condition {
      test     = "StringEquals"
      variable = "${replace(data.aws_eks_cluster.eks_cluster.identity[0].oidc[0].issuer, "https://", "")}:sub"
      values   = ["system:serviceaccount:${var.namespace}:${var.service_account_name}"]
    }

    condition {
      test     = "StringEquals"
      variable = "${replace(data.aws_eks_cluster.eks_cluster.identity[0].oidc[0].issuer, "https://", "")}:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

data "aws_iam_policy_document" "bedrock" {
  statement {
    sid     = "AllowBedrock"
    effect  = "Allow"
    actions = [
      "bedrock:InvokeModel",
      "bedrock:ListFoundationModels",
      "bedrock:ListCustomModels",
      "bedrock:GetCustomModel",
      "bedrock:Converse"  
    ]
    resources = ["*"]
  }
}

resource "aws_iam_role_policy" "bedrock_policy" {
  name   = "${local.cluster_name}-bedrock-policy"
  role   = aws_iam_role.bedrock.name
  policy = data.aws_iam_policy_document.bedrock.json
}
