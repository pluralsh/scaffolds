resource "google_service_account" "telemetry" {
  project      = local.project_id
  account_id   = "telemetry-${substr(md5(local.cluster_name), 0, 8)}"
  display_name = "${local.cluster_name} Telemetry"
  description  = "Workload identity for Plural Telemetry on ${local.cluster_name}"
}

# Every product service account impersonates the same Google service account.
resource "google_service_account_iam_member" "workload_identity" {
  for_each = local.service_accounts

  service_account_id = google_service_account.telemetry.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${local.project_id}.svc.id.goog[${local.service_account_namespace}/${each.key}]"
}

# Telemetry only needs object CRUD and listing on its own bucket.
resource "google_storage_bucket_iam_member" "telemetry" {
  bucket = google_storage_bucket.telemetry.name
  role   = "roles/storage.objectUser"
  member = "serviceAccount:${google_service_account.telemetry.email}"
}
