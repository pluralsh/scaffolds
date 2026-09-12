output "storage_account_name" {
  description = "Azure Storage account hosting Kestra internal storage."
  value       = data.azurerm_storage_account.kestra.name
}

output "storage_account_endpoint" {
  description = "Azure Blob service endpoint for Kestra internal storage."
  value       = trimsuffix(data.azurerm_storage_account.kestra.primary_blob_endpoint, "/")
}

output "storage_bucket_name" {
  description = "Azure Blob container used for Kestra internal storage."
  value       = azurerm_storage_container.kestra.name
}

output "service_account_name" {
  description = "Kubernetes service account linked through Azure Workload Identity."
  value       = local.service_account_name
}

output "azure_client_id" {
  description = "Client ID of the Kestra user-assigned identity."
  value       = azurerm_user_assigned_identity.kestra.client_id
}

output "postgres_host" {
  description = "Private Azure PostgreSQL hostname."
  value       = azurerm_postgresql_flexible_server.kestra.fqdn
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
