data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name              = local.cluster_context.cluster_name
  service_account_name      = "tempo"
  service_account_namespace = "tempo"
}

resource "aws_iam_role" "tempo" {
  name = "${local.cluster_name}-tempo"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid    = "AllowEksAuthToAssumeRoleForPodIdentity"
      Effect = "Allow"
      Principal = {
        Service = "pods.eks.amazonaws.com"
      }
      Action = [
        "sts:AssumeRole",
        "sts:TagSession",
      ]
    }]
  })
}

resource "aws_iam_policy" "tempo" {
  name_prefix = "tempo-"
  description = "Least-privilege access to Tempo trace storage"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid    = "TempoPermissions"
      Effect = "Allow"
      Action = [
        "s3:PutObject",
        "s3:GetObject",
        "s3:ListBucket",
        "s3:DeleteObject",
        "s3:GetObjectTagging",
        "s3:PutObjectTagging",
      ]
      Resource = [
        aws_s3_bucket.tempo.arn,
        "${aws_s3_bucket.tempo.arn}/*",
      ]
    }]
  })
}

resource "aws_iam_role_policy_attachment" "tempo" {
  role       = aws_iam_role.tempo.name
  policy_arn = aws_iam_policy.tempo.arn
}

resource "aws_eks_pod_identity_association" "tempo" {
  cluster_name    = local.cluster_name
  namespace       = local.service_account_namespace
  service_account = local.service_account_name
  role_arn        = aws_iam_role.tempo.arn
}
