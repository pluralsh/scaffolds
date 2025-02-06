output "postgres_host" {
  value = try(module.db.db_instance_address, "")
}

output "postgres_password" {
  value = random_password.grafana.result
  sensitive = true
}

output "admin_password" {
  value = random_password.admin.result
  sensitive = true
}

output "oidc_cookie_secret" {
  value = random_password.oidc_cookie.result
  sensitive = true
}

output "oidc_client_id" {
  value = plural_oidc_provider.grafana.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.grafana.client_secret
  sensitive = true
}