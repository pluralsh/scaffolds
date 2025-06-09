resource "google_service_account" "dagster" {
  project      = local.project_id
  account_id   = "dagster-${local.cluster_name}"
  display_name = "dagster-${local.cluster_name}"
  description  = "Service account for dagster in ${local.cluster_name} cluster"
}

resource "google_service_account_iam_binding" "workload_identity_binding" {
  service_account_id = google_service_account.dagster.name
  role               = "roles/iam.workloadIdentityUser"

  members = [
    "serviceAccount:${local.project_id}.svc.id.goog[dagster/dagster]",
  ]
}