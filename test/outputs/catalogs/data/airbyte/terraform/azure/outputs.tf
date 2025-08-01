output "connection_string" {
  value = data.azurerm_storage_account.airbyte.primary_connection_string
  sensitive = true
}

output "postgres_host" {
  value = azurerm_postgresql_flexible_server.postgres.fqdn
}

output "postgres_password" {
  value = random_password.db_password.result
  sensitive = true
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

output "basic_auth_username" {
  value = "airbyte"
}

output "basic_auth_password" {
  value = bcrypt(random_password.basic_auth_password.result)
  sensitive = true
}