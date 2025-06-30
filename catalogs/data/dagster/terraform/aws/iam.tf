# Permission management for Dagster pods
resource "aws_iam_role" "dagster" {
  name = "${local.cluster_name}-dagster"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AllowEksAuthToAssumeRoleForPodIdentity"
        Effect = "Allow"
        Principal = {
          Service = "pods.eks.amazonaws.com"
        }
        Action = [
          "sts:AssumeRole",
          "sts:TagSession"
        ]
      }
    ]
  })
}

resource "aws_iam_policy" "dagster" {
  name_prefix = "dagster"
  description = "policy for the dagster S3 access"
  policy      = <<-POLICY
    {
      "Version": "2012-10-17",
      "Statement": [
        {
          "Action": [
            "s3:*"
          ],
          "Effect": "Allow",
          "Resource": [
            "arn:aws:s3:::${var.dagster_bucket}",
            "arn:aws:s3:::${var.dagster_bucket}/*"
          ]
        }
      ]
    }
  POLICY
}

resource "aws_iam_role_policy_attachment" "dagster_s3_attachment" {
  role       = aws_iam_role.dagster.name
  policy_arn = aws_iam_policy.dagster.arn
}

resource "aws_eks_pod_identity_association" "dagster_pod_identity" {
  cluster_name    = local.cluster_name
  namespace       = local.service_account_namespace
  service_account = local.service_account_name
  role_arn        = aws_iam_role.dagster.arn
}
