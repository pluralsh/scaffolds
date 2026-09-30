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

data "google_project" "current" {
}

# Private bucket holding the function packages that Cloud Run deploys without a build.
resource "google_storage_bucket" "functions" {
  name                        = local.bucket_name
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = true
  labels                      = var.labels
}

# Cloud Run's service agent reads the packages when it deploys a revision.
resource "google_storage_bucket_iam_member" "run_agent" {
  bucket = google_storage_bucket.functions.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:service-${data.google_project.current.number}@serverless-robot-prod.iam.gserviceaccount.com"
}

# The package is read from the directory the stack's init container downloads the release
# to. The version is part of the object name, so a new release deploys a new revision.
resource "google_storage_bucket_object" "function" {
  for_each = local.functions

  name   = "${each.value.binary}/${var.artifact_version}.tar.gz"
  bucket = google_storage_bucket.functions.name
  source = local.artifacts[each.key]

  lifecycle {
    precondition {
      condition     = fileexists(local.artifacts[each.key])
      error_message = "${local.artifacts[each.key]} not found. The stack's fetch-functions init container downloads it; check that it ran and that ${var.artifact_version} contains ${each.value.binary}.tar.gz."
    }
  }
}

# One service per function, so each function runs as its own service account with only the
# permissions it needs. Ingress is public but no allUsers invoker binding exists, so only
# identities granted roles/run.invoker can call it. The package runs on Cloud Run's OS-only
# base image without a container build, which needs the google-beta provider.
resource "google_cloud_run_v2_service" "function" {
  provider = google-beta
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
      image          = "scratch"
      base_image_uri = "${var.region}-docker.pkg.dev/serverless-runtimes/google-24/runtimes/osonly24"
      command        = ["./function"]

      dynamic "env" {
        for_each = each.value.environment

        content {
          name  = env.key
          value = env.value
        }
      }

      source_code {
        cloud_storage_source {
          bucket     = google_storage_bucket.functions.name
          object     = google_storage_bucket_object.function[each.key].name
          generation = google_storage_bucket_object.function[each.key].generation
        }
      }

      resources {
        cpu_idle = true
        limits = {
          cpu    = "1"
          memory = each.value.memory
        }
      }
    }
  }

  depends_on = [google_project_service.run, google_storage_bucket_iam_member.run_agent]
}
