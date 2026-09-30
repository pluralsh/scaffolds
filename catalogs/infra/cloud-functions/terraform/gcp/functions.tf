locals {
  # Cloud Functions builds the source with Cloud Build into an image in Artifact Registry and
  # runs it on Cloud Run, so all four APIs are needed.
  apis = toset([
    "cloudfunctions.googleapis.com",
    "run.googleapis.com",
    "cloudbuild.googleapis.com",
    "artifactregistry.googleapis.com",
  ])
}

resource "google_project_service" "api" {
  for_each = local.apis

  service            = each.value
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

# Cloud Build builds the functions as this service account instead of the Compute Engine
# default service account, which is broadly privileged (it is often granted Editor). It gets
# the roles Google documents for a custom Cloud Functions build service account
# (https://cloud.google.com/functions/docs/building): logging.logWriter for the build logs,
# artifactregistry.writer for the built images, and storage.objectViewer for the source, which
# is granted on the source bucket only.
resource "google_service_account" "build" {
  account_id   = local.build_service_account
  display_name = "${var.name} operational functions build"
}

resource "google_project_iam_member" "build" {
  for_each = toset(["roles/logging.logWriter", "roles/artifactregistry.writer"])

  project = local.project_id
  role    = each.value
  member  = "serviceAccount:${google_service_account.build.email}"
}

# Private bucket holding the function source that Cloud Build reads.
resource "google_storage_bucket" "functions" {
  name                        = local.bucket_name
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = true
  labels                      = var.labels
}

resource "google_storage_bucket_iam_member" "build" {
  bucket = google_storage_bucket.functions.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.build.email}"
}

data "google_project" "current" {
}

# The source is read from the directory the stack's init container downloads the release to.
# It holds the Go module of every GCP function, and the version is part of the object name, so
# a new release uploads a new object and redeploys the functions.
resource "google_storage_bucket_object" "source" {
  name   = "functions-gcp/${var.artifact_version}.zip"
  bucket = google_storage_bucket.functions.name
  source = local.artifact

  lifecycle {
    precondition {
      condition     = fileexists(local.artifact)
      error_message = "${local.artifact} not found. The stack's fetch-functions init container downloads it; check that it ran and that ${var.artifact_version} contains functions-gcp.zip."
    }
  }
}

# One Cloud Run function (2nd gen) per function, so each runs as its own service account with
# only the permissions it needs. Ingress is public but no allUsers invoker binding exists, so
# only identities granted roles/run.invoker (see workbench.tf) can call it.
resource "google_cloudfunctions2_function" "function" {
  for_each = local.functions

  name     = local.service_names[each.key]
  location = var.region
  labels   = var.labels

  build_config {
    runtime         = "go127"
    entry_point     = each.value.entry_point
    service_account = "projects/${local.project_id}/serviceAccounts/${google_service_account.build.email}"

    source {
      storage_source {
        bucket     = google_storage_bucket.functions.name
        object     = google_storage_bucket_object.source.name
        generation = google_storage_bucket_object.source.generation
      }
    }
  }

  service_config {
    service_account_email          = google_service_account.function[each.key].email
    available_memory               = each.value.memory
    timeout_seconds                = each.value.timeout
    max_instance_count             = var.max_instance_count
    ingress_settings               = "ALLOW_ALL"
    all_traffic_on_latest_revision = true
    environment_variables          = each.value.environment
  }

  depends_on = [
    google_project_service.api,
    google_project_iam_member.build,
    google_storage_bucket_iam_member.build,
  ]
}
