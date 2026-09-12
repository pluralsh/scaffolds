output "project_id" {
  description = "GCP project hosting the Kestra infrastructure."
  value       = local.project_id
}

output "storage_bucket_name" {
  description = "GCS bucket used for Kestra internal storage."
  value       = google_storage_bucket.kestra.name
}

output "service_account_name" {
  description = "Kubernetes service account linked through GKE Workload Identity."
  value       = local.service_account_name
}

output "gcp_service_account_email" {
  description = "Google service account used by Kestra."
  value       = google_service_account.kestra.email
}

output "postgres_host" {
  description = "Private Cloud SQL address."
  value       = module.database.private_ip_address
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
