output "access_key_id" {
  value = aws_iam_access_key.osbuilder.id
}

output "secret_access_key" {
  value = aws_iam_access_key.osbuilder.secret
  sensitive = true
}

output "bucket_name" {
  value = aws_s3_bucket.osbuilder.bucket
}

output "bucket_domain" {
  value = aws_s3_bucket.osbuilder.bucket_regional_domain_name
}

output "bucket_region" {
  value = aws_s3_bucket.osbuilder.region
}