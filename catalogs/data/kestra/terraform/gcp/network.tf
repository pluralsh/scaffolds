data "google_compute_network" "network" {
  name    = local.network_short
  project = local.project_id

  depends_on = [data.plural_service_context.network]
}
