resource "google_service_account" "kestra" {
  project      = local.project_id
  account_id   = local.resource_name
  display_name = "Kestra on ${local.cluster_name}"
  description  = "Workload identity for Kestra internal storage"
}

resource "google_service_account_iam_member" "workload_identity" {
  service_account_id = google_service_account.kestra.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${local.project_id}.svc.id.goog[${local.service_account_namespace}/${local.service_account_name}]"
}
