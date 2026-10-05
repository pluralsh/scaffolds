data "plural_cloud_connection" "workbench" {
  name = var.cloud_connection

  lifecycle {
    postcondition {
      condition     = self.cloud_provider == "GCP"
      error_message = "Cloud connection ${var.cloud_connection} is for ${self.cloud_provider}, not GCP."
    }
  }
}

# One Cloud Run function per deployed function, each with its own service account, custom role
# and workbench tool, see modules/function. They share the build setup in build.tf.
module "function" {
  source   = "./modules/function"
  for_each = local.functions

  name     = "${var.name}-${each.key}"
  function = each.value
  # Service account IDs are limited to 30 characters, custom role IDs to 64 letters, digits,
  # underscores and dots. Both are cut to fit, so the function's own hash keeps them unique.
  account_id = "${trim(substr("${var.name}-${each.key}", 0, 23), "-")}-${local.function_hash[each.key]}"
  role_id    = "${substr(replace("${var.name}_${each.key}", "-", "_"), 0, 57)}_${local.function_hash[each.key]}"

  project_id         = local.project_id
  region             = var.region
  labels             = var.labels
  max_instance_count = var.max_instance_count

  build = {
    service_account_email = google_service_account.build.email
    docker_repository     = google_artifact_registry_repository.functions.id
    bucket                = google_storage_bucket.functions.name
    object                = google_storage_bucket_object.source.name
    generation            = google_storage_bucket_object.source.generation
  }

  invoker_service_account = var.invoker_service_account
  cloud_connection_id     = data.plural_cloud_connection.workbench.id

  # The build service account needs its grants before Cloud Build runs.
  depends_on = [
    google_project_service.api,
    google_project_iam_member.build_logs,
    google_artifact_registry_repository_iam_member.build,
    google_storage_bucket_iam_member.build,
  ]
}
