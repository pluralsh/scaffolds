resource "google_service_account" "dagster" {
  project      = local.project_id
  account_id   = "${local.cluster_name}-dagster"
  display_name = "${local.cluster_name}-dagster"
  description  = "Service account for dagster in ${local.cluster_name} cluster"
}

resource "google_service_account_iam_binding" "workload_identity_binding" {
  service_account_id = google_service_account.dagster.name
  role               = "roles/iam.workloadIdentityUser"

  members = [
    "serviceAccount:${local.project_id}.svc.id.goog[${local.service_account_name}/${local.service_account_namespace}]",
  ]
}