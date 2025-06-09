resource "google_storage_bucket" "dagster" {
  name          = var.dagster_bucket
  project       = local.project_id
  location      = local.region
  force_destroy = var.force_destroy_bucket

  uniform_bucket_level_access = true

  versioning {
    enabled = true
  }

  lifecycle_rule {
    action {
      type = "Delete"
    }
    condition {
      age = 30
    }
  }
}

resource "google_storage_bucket_iam_member" "default" {
  bucket = google_storage_bucket.dagster.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.dagster.email}"
}