output "bucket_name" {
  value = aws_s3_bucket.telemetry.bucket
}

output "region" {
  value = var.region
}
