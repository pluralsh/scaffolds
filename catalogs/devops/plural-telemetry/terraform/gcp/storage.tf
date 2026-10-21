data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  project_id                = local.cluster_context.project_id
  region                    = local.cluster_context.region
  cluster_name              = local.cluster_context.cluster_name
  service_accounts          = toset(["metrics", "logs", "traces"])
  service_account_namespace = "plrl-telemetry"
}

resource "google_storage_bucket" "telemetry" {
  name                        = var.bucket_name
  project                     = local.project_id
  location                    = local.region
  force_destroy               = var.force_destroy_bucket
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"

  versioning {
    enabled = var.enable_versioning
  }

  # Kept even with versioning off, so versions stored while it was on still expire.
  lifecycle_rule {
    action {
      type = "Delete"
    }

    condition {
      days_since_noncurrent_time = var.version_retention_days
    }
  }

  lifecycle_rule {
    action {
      type = "AbortIncompleteMultipartUpload"
    }

    condition {
      age = 1
    }
  }
}
