# Shared by every function: the APIs, and the pipeline that turns the release's Go source into
# the image each Cloud Run function runs. Cloud Functions builds the source with Cloud Build
# into an Artifact Registry image and runs it on Cloud Run.

locals {
  apis = toset([
    "cloudfunctions.googleapis.com",
    "run.googleapis.com",
    "cloudbuild.googleapis.com",
    "artifactregistry.googleapis.com",
  ])

  # Every function is an entry point of the same Go source package.
  artifact = "${var.artifact_dir}/${var.artifact_version}/functions-gcp.zip"
  # Artifact Registry repository IDs are unique per project and region.
  repository_id = "${var.name}-functions"
  # Bucket names are globally unique.
  bucket_name = "plrl-fn-${substr(sha1("${local.project_id}/${var.name}"), 0, 16)}"
}

resource "google_project_service" "api" {
  for_each = local.apis

  service            = each.value
  disable_on_destroy = false
}

# Holds the images Cloud Build builds. Used instead of the gcf-artifacts repository Cloud
# Functions would create, so the build service account can write only here. Images older than
# release_retention_days are deleted, except the 5 most recent versions of each function, so
# the deployed image is always kept.
resource "google_artifact_registry_repository" "functions" {
  repository_id = local.repository_id
  location      = var.region
  format        = "DOCKER"
  description   = "Images of the ${var.name} operational functions."
  labels        = var.labels

  cleanup_policies {
    id     = "delete-old"
    action = "DELETE"
    condition {
      tag_state  = "ANY"
      older_than = "${var.release_retention_days * 86400}s"
    }
  }

  cleanup_policies {
    id     = "keep-recent"
    action = "KEEP"
    most_recent_versions {
      keep_count = 5
    }
  }

  depends_on = [google_project_service.api]
}

# Private bucket for the function source Cloud Build reads. Each release overwrites the same
# object; with versioning, earlier sources become noncurrent and are deleted after
# release_retention_days. The current source is never deleted. Soft delete is off, since the
# source can always be downloaded from the release again.
resource "google_storage_bucket" "functions" {
  name                        = local.bucket_name
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = true
  labels                      = var.labels

  versioning {
    enabled = true
  }

  soft_delete_policy {
    retention_duration_seconds = 0
  }

  lifecycle_rule {
    condition {
      with_state                 = "ARCHIVED"
      days_since_noncurrent_time = var.release_retention_days
    }
    action {
      type = "Delete"
    }
  }
}

# The source comes from the release the stack's init container downloads, and holds the Go
# module of every GCP function. New source gives the object a new generation. The functions
# pin the generation, so they are rebuilt from exactly that upload.
resource "google_storage_bucket_object" "source" {
  name   = "functions-gcp.zip"
  bucket = google_storage_bucket.functions.name
  source = local.artifact

  metadata = {
    release = var.artifact_version
  }

  lifecycle {
    precondition {
      condition     = fileexists(local.artifact)
      error_message = "${local.artifact} not found. The stack's fetch-functions init container downloads it; check that it ran and that ${var.artifact_version} contains functions-gcp.zip."
    }
  }
}
