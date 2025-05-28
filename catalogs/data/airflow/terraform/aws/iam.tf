locals {
  cluster_name = jsondecode(data.plural_service_context.mgmt.configuration)["cluster_name"]
}

data "aws_eks_cluster" "cluster" {
  name = local.eks_cluster_name
}

resource "aws_iam_policy" "airflow" {
  name_prefix = "airflow"
  description = "policy for the plural admin airflow"
  policy      = data.aws_iam_policy_document.airflow.json
}

resource "aws_iam_user" "airflow" {
  name = "${local.cluster_name}-airflow"
}

resource "aws_iam_access_key" "airflow" {
  user = aws_iam_user.airflow.name
}

data "aws_iam_policy_document" "airflow" {
  statement {
    sid    = "admin"
    effect = "Allow"
    actions = ["s3:*"]

    resources = [
      "arn:aws:s3:::${var.airflow_bucket}",
      "arn:aws:s3:::${var.airflow_bucket}/*",
    ]
  }
}

resource "aws_iam_policy_attachment" "airflow-user" {
  name = "${local.cluster_name}-airflow-policy"
  users = [aws_iam_user.airflow.name]
  policy_arn = aws_iam_policy.airflow.arn
}

module "assumable_role_bedrock" {
  source  = "terraform-aws-modules/iam/aws//modules/iam-assumable-role-with-oidc"
  version = "5.52.2"

  create_role      = true
  role_name        = "${local.eks_cluster_name}-bedrock"
  provider_url     = replace(data.aws_eks_cluster.cluster.identity[0].oidc[0].issuer, "https://", "")
  role_policy_arns = [aws_iam_policy.airflow.arn]

  oidc_fully_qualified_subjects = [
    "system:serviceaccount:airflow:airflow",
  ]
}