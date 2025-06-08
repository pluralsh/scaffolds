data "google_compute_network" "network" {
  name = local.network
  project = local.project_id

  depends_on = [
    data.plural_service_context.network
  ]
}

resource "random_string" "airbyte_address_suffix" {
  length  = 6
  special = false
  upper   = false
}

resource "google_compute_global_address" "private_ip_alloc" {
  name          = "airbyte-${var.cluster_name}-${random_string.airbyte_address_suffix.result}"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = data.google_compute_network.network.id
  project       = local.project_id
}


resource "google_service_networking_connection" "postgres" {
  network                 = data.google_compute_network.network.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip_alloc.name]
  update_on_creation_fail = true
  deletion_policy         = "ABANDON"
}