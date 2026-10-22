data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name              = local.cluster_context.cluster_name
  service_accounts          = toset(["metrics", "logs", "traces"])
  service_account_namespace = "plrl-telemetry"
}

resource "aws_iam_role" "telemetry" {
  name = "${local.cluster_name}-telemetry"

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

resource "aws_iam_policy" "telemetry" {
  name_prefix = "telemetry-"
  description = "Least-privilege access to Telemetry object storage"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "TelemetryBucket"
        Effect = "Allow"
        Action = [
          "s3:ListBucket",
          "s3:ListBucketMultipartUploads",
        ]
        Resource = aws_s3_bucket.telemetry.arn
      },
      {
        Sid    = "TelemetryObjects"
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:PutObject",
          "s3:DeleteObject",
          "s3:AbortMultipartUpload",
        ]
        Resource = "${aws_s3_bucket.telemetry.arn}/*"
      },
    ]
  })
}

resource "aws_iam_role_policy_attachment" "telemetry" {
  role       = aws_iam_role.telemetry.name
  policy_arn = aws_iam_policy.telemetry.arn
}

# One association per product service account; they all share the same bucket.
resource "aws_eks_pod_identity_association" "telemetry" {
  for_each = local.service_accounts

  cluster_name    = local.cluster_name
  namespace       = local.service_account_namespace
  service_account = each.key
  role_arn        = aws_iam_role.telemetry.arn
}
