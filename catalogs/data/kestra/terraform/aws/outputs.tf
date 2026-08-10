output "storage_bucket_name" {
  description = "S3 bucket used for Kestra internal storage."
  value       = aws_s3_bucket.kestra.bucket
}

output "service_account_name" {
  description = "Kubernetes service account associated with EKS Pod Identity."
  value       = local.service_account_name
}

output "postgres_host" {
  description = "Private RDS hostname."
  value       = module.database.db_instance_address
}

output "postgres_password" {
  description = "Generated Kestra database password."
  value       = random_password.database.result
  sensitive   = true
}

output "admin_password" {
  description = "Generated password for the initial Kestra administrator."
  value       = random_password.admin.result
  sensitive   = true
}

output "encryption_key" {
  description = "Generated 256-bit Kestra encryption key, encoded as base64."
  value       = random_id.encryption_key.b64_std
  sensitive   = true
}
