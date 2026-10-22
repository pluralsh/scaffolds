output "client_id" {
  value = azurerm_user_assigned_identity.telemetry.client_id
}

output "tenant_id" {
  value = local.identity_context.tenant_id
}

output "container_name" {
  value = azurerm_storage_container.telemetry.name
}

output "storage_account_name" {
  value = data.azurerm_storage_account.telemetry.name
}
