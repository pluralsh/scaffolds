resource "google_project_service" "run" {
  service            = "run.googleapis.com"
  disable_on_destroy = false
}

resource "google_service_account" "function" {
  for_each = local.functions

  account_id   = local.service_accounts[each.key]
  display_name = "${local.service_names[each.key]} operational function"
}

resource "google_project_iam_custom_role" "function" {
  for_each = { for key, fn in local.functions : key => fn if length(fn.permissions) > 0 }

  role_id     = local.role_ids[each.key]
  title       = "${local.service_names[each.key]} operational function"
  permissions = each.value.permissions
}

resource "google_project_iam_member" "function" {
  for_each = google_project_iam_custom_role.function

  project = local.project_id
  role    = each.value.name
  member  = "serviceAccount:${google_service_account.function[each.key].email}"
}

# One service per function, so each function runs as its own service account with only the
# permissions it needs. Ingress is public but no allUsers invoker binding exists, so only
# identities granted roles/run.invoker can call it.
resource "google_cloud_run_v2_service" "function" {
  for_each = local.functions

  name                = local.service_names[each.key]
  location            = var.region
  ingress             = "INGRESS_TRAFFIC_ALL"
  deletion_protection = false
  labels              = var.labels

  template {
    service_account = google_service_account.function[each.key].email
    timeout         = each.value.timeout

    scaling {
      max_instance_count = var.max_instance_count
    }

    containers {
      image = "${coalesce(var.image_repository, "unset")}/${each.value.binary}:${var.artifact_version}"

      resources {
        cpu_idle = true
        limits = {
          cpu    = "1"
          memory = each.value.memory
        }
      }
    }
  }

  lifecycle {
    precondition {
      condition     = var.image_repository != null
      error_message = "image_repository must be set until the public Plural function images are published."
    }
  }

  depends_on = [google_project_service.run]
}
