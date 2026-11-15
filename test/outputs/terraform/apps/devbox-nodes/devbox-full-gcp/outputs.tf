output "cluster_name" {
  value = local.cluster_context.cluster_name
}

output "node_pool" {
  value = google_container_node_pool.devbox.name
}
