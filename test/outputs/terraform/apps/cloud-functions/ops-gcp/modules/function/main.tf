# Everything a single function deploys: the Cloud Run function (2nd gen) and its workbench tool
# here, its own service account with only the permissions it needs and who may invoke it in
# iam.tf. The build setup and source are shared by all functions, see build.tf and iam.tf in
# the parent.

# Ingress is public, but there is no allUsers invoker binding, so only identities with
# roles/run.invoker can call it.
resource "google_cloudfunctions2_function" "function" {
  name     = var.name
  location = var.region
  labels   = var.labels

  build_config {
    runtime           = "go127"
    entry_point       = var.function.entry_point
    service_account   = "projects/${var.project_id}/serviceAccounts/${var.build.service_account_email}"
    docker_repository = var.build.docker_repository

    source {
      storage_source {
        bucket     = var.build.bucket
        object     = var.build.object
        generation = var.build.generation
      }
    }
  }

  service_config {
    service_account_email          = google_service_account.function.email
    available_memory               = var.function.memory
    timeout_seconds                = var.function.timeout
    max_instance_count             = var.max_instance_count
    ingress_settings               = "ALLOW_ALL"
    all_traffic_on_latest_revision = true
    environment_variables          = merge(var.function.environment, { GOOGLE_CLOUD_PROJECT = var.project_id })
  }
}

locals {
  # Name of the Cloud Run service behind the function: the last segment of
  # projects/<project>/locations/<region>/services/<name>.
  run_service = reverse(split("/", google_cloudfunctions2_function.function.service_config[0].service))[0]
}

resource "plural_workbench_tool" "function" {
  name                = var.tool_name
  tool                = "CLOUD_RUN"
  cloud_connection_id = var.cloud_connection_id
  approval            = var.function.destructive

  configuration = {
    cloud_run = {
      identifier   = google_cloudfunctions2_function.function.service_config[0].uri
      description  = var.function.description
      input_schema = var.function.schema
    }
  }
}
