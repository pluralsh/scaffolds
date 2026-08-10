resource "google_storage_bucket" "kestra" {
  name                        = var.storage_bucket
  project                     = local.project_id
  location                    = local.region
  force_destroy               = var.force_destroy_bucket
  uniform_bucket_level_access = true

  versioning {
    enabled = true
  }
}

resource "google_storage_bucket_iam_member" "kestra_objects" {
  bucket = google_storage_bucket.kestra.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.kestra.email}"
}
