locals {
  substrate = var.install_substrate ? 1 : 0
  # Each reads and writes the bucket through the default AWS credential chain, here EKS Pod Identity: ate-api-server
  # and atelet for snapshots, devbox-zerofs (the ZeroFS controller and node plugin) for devbox homes.
  substrate_service_accounts = var.install_substrate ? toset(["ate-api-server", "atelet", "devbox-zerofs"]) : toset([])
}

resource "aws_s3_bucket" "substrate" {
  count         = local.substrate
  bucket_prefix = substr("${local.cluster_name}-ate-", 0, 37)
}

resource "aws_s3_bucket_public_access_block" "substrate" {
  count                   = local.substrate
  bucket                  = aws_s3_bucket.substrate[0].id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

data "aws_iam_policy_document" "substrate_assume" {
  statement {
    actions = ["sts:AssumeRole", "sts:TagSession"]
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
  }
}

data "aws_iam_policy_document" "substrate_bucket" {
  count = local.substrate

  statement {
    actions   = ["s3:ListBucket", "s3:GetBucketLocation", "s3:ListBucketMultipartUploads"]
    resources = [aws_s3_bucket.substrate[0].arn]
  }

  statement {
    actions = [
      "s3:GetObject",
      "s3:PutObject",
      "s3:DeleteObject",
      "s3:AbortMultipartUpload",
      "s3:ListMultipartUploadParts",
    ]
    resources = ["${aws_s3_bucket.substrate[0].arn}/*"]
  }
}

resource "aws_iam_role" "substrate" {
  for_each           = local.substrate_service_accounts
  name               = substr("${local.cluster_name}-${each.key}", 0, 64)
  assume_role_policy = data.aws_iam_policy_document.substrate_assume.json
}

resource "aws_iam_role_policy" "substrate" {
  for_each = local.substrate_service_accounts
  name     = "substrate-snapshots"
  role     = aws_iam_role.substrate[each.key].id
  policy   = data.aws_iam_policy_document.substrate_bucket[0].json
}

resource "aws_eks_pod_identity_association" "substrate" {
  for_each        = local.substrate_service_accounts
  cluster_name    = local.cluster_name
  namespace       = "ate-system"
  service_account = each.key
  role_arn        = aws_iam_role.substrate[each.key].arn
}
