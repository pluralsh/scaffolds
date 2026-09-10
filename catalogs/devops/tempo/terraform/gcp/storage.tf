data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  project_id                = local.cluster_context.project_id
  region                    = local.cluster_context.region
  cluster_name              = local.cluster_context.cluster_name
  service_account_name      = "tempo"
  service_account_namespace = "tempo"
}

resource "google_storage_bucket" "tempo" {
  name                        = var.bucket_name
  project                     = local.project_id
  location                    = local.region
  force_destroy               = var.force_destroy_bucket
  uniform_bucket_level_access = true

  versioning {
    enabled = true
  }

  lifecycle_rule {
    action {
      type = "Delete"
    }

    condition {
      days_since_noncurrent_time = 7
    }
  }
}
