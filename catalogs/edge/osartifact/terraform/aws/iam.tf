# Create IAM Role
resource "aws_iam_role" "ecr_push_role" {
  name               = "${data.plural_cluster.cluster.name}-plrl-osartifact-${var.osartifact_name}-assume"
  assume_role_policy = data.aws_iam_policy_document.ecr_assume_role_policy.json
}

# Trust policy for the IAM role (allows EKS service account to assume the role)
data "aws_iam_policy_document" "ecr_assume_role_policy" {
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

# Trust policy for the IAM role
data "aws_iam_policy_document" "ecr_push_policy" {
  statement {
    actions = ["ecr:GetAuthorizationToken"]
    resources = ["*"]
  }

  statement {
    actions = ["ecr:*"]
    resources = [
      data.aws_ecr_repository.ecr_repository.arn,
    ]
  }
}

# Attach the ECR permissions to the IAM Role
resource "aws_iam_role_policy" "ecr_push_policy" {
  name               = "${data.plural_cluster.cluster.name}-plrl-osartifact-${var.osartifact_name}-push"
  role               = aws_iam_role.ecr_push_role.id
  policy             = data.aws_iam_policy_document.ecr_push_policy.json
}