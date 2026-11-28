output "cluster_name" {
  value = local.cluster_name
}

# the devbox NodePool carries the Cilium label and startup taint NAP expects on Cilium clusters
output "network_dataplane" {
  value = try(data.azapi_resource.cluster.output.properties.networkProfile.networkDataplane, "")
}

output "node_pools" {
  value = [for p in azurerm_kubernetes_cluster_node_pool.devbox : p.name]
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
