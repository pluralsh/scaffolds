data "google_client_config" "provider" {}

data "google_container_cluster" "cluster" {
  project  = local.project_id
  name     = local.cluster_name
  location = local.region
}

resource "kubernetes_service_account" "dagster" {
  metadata {
    name      = local.service_account_name
    namespace = local.service_account_namespace
    annotations = {
      "iam.gke.io/gcp-service-account" = google_service_account.dagster.email
    }
  }
}
