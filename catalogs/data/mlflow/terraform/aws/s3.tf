resource "aws_s3_bucket" "mlflow" {
  bucket         = var.mlflow_bucket
  force_destroy  = var.force_destroy_bucket
}

resource "aws_s3_bucket_server_side_encryption_configuration" "mlflow" {
  bucket = aws_s3_bucket.mlflow.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}
