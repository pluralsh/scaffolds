resource "aws_s3_bucket" "airflow" {
  bucket         = var.airflow_bucket
  force_destroy  = var.force_destroy_bucket
}

resource "aws_s3_bucket_server_side_encryption_configuration" "airflow" {
  bucket = aws_s3_bucket.airflow.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}
