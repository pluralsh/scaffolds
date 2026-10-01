# Identities and grants shared by every function. Each function's own service account, custom
# role and invoker binding are in modules/function/iam.tf.

locals {
  # The name is capped at 17 characters so the ID, with "-build-" and the hash, stays within 30.
  build_service_account = "${trim(substr(var.name, 0, 17), "-")}-build-${local.hash}"
}

# Cloud Build runs as this service account instead of the Compute Engine default one, which is
# broadly privileged (often granted Editor). It gets the roles Google documents for a custom
# build service account (https://cloud.google.com/functions/docs/building): logging.logWriter
# for build logs (the only project-level grant), artifactregistry.writer on the functions' own
# repository, and storage.objectViewer on the source bucket.
resource "google_service_account" "build" {
  account_id   = local.build_service_account
  display_name = "${var.name} operational functions build"
}

resource "google_project_iam_member" "build_logs" {
  project = local.project_id
  role    = "roles/logging.logWriter"
  member  = "serviceAccount:${google_service_account.build.email}"
}

resource "google_artifact_registry_repository_iam_member" "build" {
  repository = google_artifact_registry_repository.functions.name
  location   = google_artifact_registry_repository.functions.location
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.build.email}"
}

resource "google_storage_bucket_iam_member" "build" {
  bucket = google_storage_bucket.functions.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.build.email}"
}
