output "cluster_name" {
  value = local.cluster_context.cluster_name
}

output "node_pool" {
  value = google_container_node_pool.devbox.name
}

output "substrate_snapshot_location" {
  value = try("gs://${google_storage_bucket.substrate[0].name}/substrate/", "")
}

output "substrate_api_server_service_account" {
  value = try(google_service_account.substrate_api_server[0].email, "")
}

output "substrate_transfer_service_account" {
  value = try(google_service_account.substrate_transfer[0].email, "")
}

output "substrate_zerofs_service_account" {
  value = try(google_service_account.substrate_zerofs[0].email, "")
}
