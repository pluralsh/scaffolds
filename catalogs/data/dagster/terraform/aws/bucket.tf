resource "aws_s3_bucket" "dagster" {
  bucket         = "${local.cluster_name}-dagster"
  force_destroy  = var.force_destroy_bucket
}

resource "aws_s3_bucket_server_side_encryption_configuration" "dagster" {
  bucket = aws_s3_bucket.dagster.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}
