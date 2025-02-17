resource "aws_s3_bucket" "osbuilder" {
  bucket         = var.osbuilder_bucket
  force_destroy  = var.force_destroy_bucket
}

resource "aws_s3_bucket_server_side_encryption_configuration" "osbuilder" {
  bucket = aws_s3_bucket.osbuilder.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}
