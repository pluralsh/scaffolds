resource "google_cloud_run_v2_service_iam_member" "invoker" {
  for_each = { for key, _ in local.tools : key => google_cloud_run_v2_service.function[key] if var.invoker_service_account != null }

  name     = each.value.name
  location = each.value.location
  role     = "roles/run.invoker"
  member   = "serviceAccount:${var.invoker_service_account}"
}

# The provider does not support `approval` yet, so every tool is invokable without human
# approval. Only register functions that change nothing until that is supported.
# TODO(PROD-5251): add `approval` to plural_workbench_tool in terraform-provider-plural and
# set it here for every function that changes resources.
# TODO(PROD-5251): fix the plural_cloud_connection data source (cloud_provider and
# configuration are required, so it cannot look connections up by name) and accept a
# connection name instead of cloud_connection_id.
resource "plural_workbench_tool" "function" {
  for_each = local.tools

  name                = replace(local.service_names[each.key], "-", "_")
  tool                = "CLOUD_RUN"
  cloud_connection_id = var.cloud_connection_id

  configuration = {
    cloud_run = {
      identifier   = google_cloud_run_v2_service.function[each.key].uri
      description  = each.value.description
      input_schema = each.value.schema
    }
  }
}
