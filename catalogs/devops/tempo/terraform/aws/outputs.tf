output "bucket_name" {
  value = aws_s3_bucket.tempo.bucket
}

output "region" {
  value = var.region
}

output "service_account_name" {
  value = local.service_account_name
}
