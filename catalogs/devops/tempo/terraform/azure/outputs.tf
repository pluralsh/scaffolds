output "client_id" {
  value = azurerm_user_assigned_identity.tempo.client_id
}

output "container_name" {
  value = azurerm_storage_container.tempo.name
}

output "service_account_name" {
  value = local.service_account_name
}

output "storage_account_name" {
  value = data.azurerm_storage_account.tempo.name
}
