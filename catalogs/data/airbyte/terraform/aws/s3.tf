resource "aws_s3_bucket" "airbyte" {
  bucket         = var.airbyte_bucket
  force_destroy  = var.force_destroy_bucket
}

resource "aws_s3_bucket_server_side_encryption_configuration" "airbyte" {
  bucket = aws_s3_bucket.airbyte.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

data "aws_iam_role" "postgres" {
  name = "${var.cluster_name}-postgres"
}
