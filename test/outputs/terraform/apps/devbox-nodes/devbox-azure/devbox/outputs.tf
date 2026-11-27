output "cluster_name" {
  value = local.cluster_name
}

output "node_pool" {
  value = azurerm_kubernetes_cluster_node_pool.devbox.name
}

output "substrate_snapshot_location" {
  value = try("azblob://${azurerm_storage_container.substrate[0].name}/substrate/", "")
}

output "substrate_account_url" {
  value = try(trimsuffix(azurerm_storage_account.substrate[0].primary_blob_endpoint, "/"), "")
}

output "substrate_tenant_id" {
  value = try(azurerm_user_assigned_identity.substrate["node"].tenant_id, "")
}

output "substrate_node_client_id" {
  value = try(azurerm_user_assigned_identity.substrate["node"].client_id, "")
}

output "substrate_control_client_id" {
  value = try(azurerm_user_assigned_identity.substrate["control"].client_id, "")
}

output "substrate_zerofs_client_id" {
  value = try(azurerm_user_assigned_identity.substrate["zerofs"].client_id, "")
}
