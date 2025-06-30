output "blobstore_client_id" {
  value = azurerm_user_assigned_identity.dagster.client_id
}

output "storage_account_name" {
  value = data.azurerm_storage_account.sa.name
}

output "service_account_name" {
  value = local.service_account_name
}

output "container_name" {
  value = azurerm_storage_container.dagster.name
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
  value = plural_oidc_provider.dagster.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.dagster.client_secret
  sensitive = true
}

output "aks_client_id" {
  value = azurerm_user_assigned_identity.dagster.client_id
}