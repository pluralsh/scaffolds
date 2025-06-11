output "project_id" {
  value = local.project_id
}

output "service_account_name" {
  value = kubernetes_service_account.dagster.metadata.0.name
}

output "storage_bucket_name" {
  value = google_storage_bucket.dagster.name
}

output "postgres_host" {
  value = module.pg.private_ip_address
}

output "postgres_password" {
  description = "The database password"
  value       = random_password.password.result
  sensitive   = true
}

output "oidc_cookie_secret" {
  value = random_password.oidc_cookie.result
  sensitive = true
}

output "oidc_client_id" {
  value = plural_oidc_provider.dagster.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.dagster.client_secret
  sensitive = true
}