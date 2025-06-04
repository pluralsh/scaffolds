output "project_id" {
  value = local.cluster_context.project_id
}

output "credentials_json" {
  value = google_service_account_key.default.private_key
  sensitive = true
}

output "credentials_json_decoded" {
  value = base64decode(google_service_account_key.default.private_key)
  sensitive = true
}

output "postgres_host" {
  value = module.pg.instance_ip_address
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
  value = plural_oidc_provider.airbyte.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.airbyte.client_secret
  sensitive = true
}