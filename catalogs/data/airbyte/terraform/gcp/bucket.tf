resource "google_service_account" "airbyte" {
  account_id = "airbyte-sa"
}

resource "google_service_account_key" "default" {
  service_account_id = google_service_account.airbyte.name
}

resource "google_storage_bucket" "airbyte" {
  name          = var.airbyte_bucket
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

  timeouts {
    create = "30m"
    update = "30m"
    delete = "30m"
  }
}

resource "google_storage_bucket_iam_member" "default" {
  bucket = var.airbyte_bucket
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.airbyte.email}"
}