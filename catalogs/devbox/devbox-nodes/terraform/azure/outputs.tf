output "cluster_name" {
  value = local.cluster_name
}

output "node_pool" {
  value = azurerm_kubernetes_cluster_node_pool.devbox.name
}
