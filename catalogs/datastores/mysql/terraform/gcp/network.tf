data "google_compute_network" "network" {
  name = local.network_context.network
  project = local.cluster_context.project_id

  depends_on = [
    data.plural_service_context.network
  ]
}

resource "google_compute_global_address" "private_ip_alloc" {
  name          = "plrl-mysql-${var.name}"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = data.google_compute_network.network.id
  project       = local.cluster_context.project_id
}


resource "google_service_networking_connection" "mysql" {
  network                 = data.google_compute_network.network.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip_alloc.name]
  update_on_creation_fail = true
  deletion_policy         = "ABANDON"
}