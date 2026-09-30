# A 2nd gen function runs as a Cloud Run service, and roles/run.invoker on that service is
# what allows calling it.
resource "google_cloud_run_v2_service_iam_member" "invoker" {
  for_each = { for key, _ in local.tools : key => local.run_services[key] if var.invoker_service_account != null }

  name     = each.value
  location = var.region
  role     = "roles/run.invoker"
  member   = "serviceAccount:${var.invoker_service_account}"
}

data "plural_cloud_connection" "workbench" {
  name = var.cloud_connection

  lifecycle {
    postcondition {
      condition     = self.cloud_provider == "GCP"
      error_message = "Cloud connection ${var.cloud_connection} is for ${self.cloud_provider}, not GCP."
    }
  }
}

# Tools of functions that change resources require human approval of every call.
resource "plural_workbench_tool" "function" {
  for_each = local.tools

  name                = replace(local.service_names[each.key], "-", "_")
  tool                = "CLOUD_RUN"
  cloud_connection_id = data.plural_cloud_connection.workbench.id
  approval            = each.value.destructive

  configuration = {
    cloud_run = {
      identifier   = google_cloudfunctions2_function.function[each.key].service_config[0].uri
      description  = each.value.description
      input_schema = each.value.schema
    }
  }
}
