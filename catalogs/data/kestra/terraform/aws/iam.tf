resource "aws_iam_role" "kestra" {
  name = local.resource_name

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AllowEksPodIdentity"
        Effect = "Allow"
        Principal = {
          Service = "pods.eks.amazonaws.com"
        }
        Action = [
          "sts:AssumeRole",
          "sts:TagSession",
        ]
      },
    ]
  })

  tags = local.tags
}

resource "aws_iam_policy" "kestra_storage" {
  name_prefix = "${local.resource_name}-storage-"
  description = "Least-privilege access to Kestra internal storage"
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid      = "BucketMetadata"
        Effect   = "Allow"
        Action   = ["s3:ListBucket", "s3:GetBucketLocation"]
        Resource = aws_s3_bucket.kestra.arn
      },
      {
        Sid      = "ObjectAccess"
        Effect   = "Allow"
        Action   = ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"]
        Resource = "${aws_s3_bucket.kestra.arn}/*"
      },
    ]
  })

  tags = local.tags
}

resource "aws_iam_role_policy_attachment" "kestra_storage" {
  role       = aws_iam_role.kestra.name
  policy_arn = aws_iam_policy.kestra_storage.arn
}

resource "aws_eks_pod_identity_association" "kestra" {
  cluster_name    = local.cluster_name
  namespace       = local.service_account_namespace
  service_account = local.service_account_name
  role_arn        = aws_iam_role.kestra.arn

  depends_on = [aws_iam_role_policy_attachment.kestra_storage]
}
