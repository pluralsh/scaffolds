resource "random_string" "airbyte_sa_suffix" {
  length  = 6
  special = false
  upper   = false
}

resource "google_service_account" "airbyte" {
  account_id = "airbyte-${var.cluster_name}-${random_string.airbyte_sa_suffix.result}"
  project    = local.project_id
}

resource "google_service_account_key" "default" {
  service_account_id = google_service_account.airbyte.name
}

resource "google_storage_bucket" "airbyte" {
  name          = var.airbyte_bucket
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
  bucket = google_storage_bucket.airbyte.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.airbyte.email}"
}