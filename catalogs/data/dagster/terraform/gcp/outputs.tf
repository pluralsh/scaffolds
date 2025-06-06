output "project_id" {
  value = local.project_id
}

output "service_account_name" {
  value = kubernetes_service_account.service_account.metadata.name
}

output "credentials_json" {
  value = google_service_account_key.default.private_key
  sensitive = true
}

output "credentials_json_decoded" {
  value = base64decode(google_service_account_key.default.private_key)
  sensitive = true
}

output "credentials_json_minified" {
  value = jsonencode(jsondecode(base64decode(google_service_account_key.default.private_key)))
  sensitive = true
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