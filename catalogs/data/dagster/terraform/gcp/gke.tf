data "google_client_config" "provider" {}

data "google_container_cluster" "cluster" {
  project  = local.project_id
  name     = local.cluster_name
  location = local.region
}

resource "kubernetes_service_account" "service_account" {
  metadata {
    name      = "dagster"
    namespace = "dagster"
    annotations = {
      "iam.gke.io/gcp-service-account" = google_service_account.gcp_service_account.email
    }
  }
}

resource "google_service_account" "gcp_service_account" {
  project      = local.project_id
  account_id   = "dagster-${local.cluster_name}"
  display_name = "dagster-${local.cluster_name}"
  description  = "Service account for dagster in ${local.cluster_name} cluster"
}

resource "google_service_account_iam_binding" "workload_identity_binding" {
  service_account_id = google_service_account.gcp_service_account.name
  role               = "roles/iam.workloadIdentityUser"

  members = [
    "serviceAccount:${local.project_id}.svc.id.goog[dagster/dagster]",
  ]
}