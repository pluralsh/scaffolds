# The function's own service account, with only the permissions it needs through a project
# custom role, and who may invoke it.

resource "google_service_account" "function" {
  account_id   = var.account_id
  display_name = "${var.name} operational function"
}

resource "google_project_iam_custom_role" "function" {
  role_id     = var.role_id
  title       = "${var.name} operational function"
  permissions = var.function.permissions
}

resource "google_project_iam_member" "function" {
  project = var.project_id
  role    = google_project_iam_custom_role.function.name
  member  = "serviceAccount:${google_service_account.function.email}"
}

# A 2nd gen function runs as a Cloud Run service; roles/run.invoker on it allows calling it.
resource "google_cloud_run_v2_service_iam_member" "invoker" {
  count = var.invoker_service_account != null ? 1 : 0

  name     = local.run_service
  location = var.region
  role     = "roles/run.invoker"
  member   = "serviceAccount:${var.invoker_service_account}"
}
