resource "google_service_account" "tempo" {
  project      = local.project_id
  account_id   = "tempo-${substr(md5(local.cluster_name), 0, 8)}"
  display_name = "${local.cluster_name} Tempo"
  description  = "Workload identity for Tempo on ${local.cluster_name}"
}

resource "google_service_account_iam_member" "workload_identity" {
  service_account_id = google_service_account.tempo.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${local.project_id}.svc.id.goog[${local.service_account_namespace}/${local.service_account_name}]"
}

# Tempo needs object CRUD plus storage.buckets.get; storage.admin is the
# narrowest predefined role that contains the full required permission set.
resource "google_storage_bucket_iam_member" "tempo" {
  bucket = google_storage_bucket.tempo.name
  role   = "roles/storage.admin"
  member = "serviceAccount:${google_service_account.tempo.email}"
}
