locals {
  # Every function that can be deployed. `permissions` is the minimal set of IAM permissions
  # the function needs, granted to its service account through a project custom role.
  catalog = {
    echo = {
      binary      = "echo-gcp"
      description = "Echoes its input together with the service account the function runs as. Changes nothing; used to verify the deployment."
      memory      = "512Mi"
      timeout     = "10s"
      # The metadata server needs no permissions.
      permissions = []
      schema      = jsonencode(jsondecode(file("${path.module}/schemas/echo.json")))
    }
  }

  ctx_mgmt   = jsondecode(data.plural_service_context.mgmt.configuration)
  project_id = local.ctx_mgmt.project_id

  functions     = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown       = setsubtract(var.functions, keys(local.catalog))
  service_names = { for key, _ in local.functions : key => "${var.name}-${key}" }
  artifacts     = { for key, fn in local.functions : key => "${var.artifact_dir}/${var.artifact_version}/${fn.binary}.tar.gz" }

  # Service account IDs are limited to 30 characters and custom role IDs to letters, digits,
  # underscores and dots, so both get a suffix derived from the installation name.
  hash             = substr(sha1("${local.project_id}/${var.name}"), 0, 6)
  service_accounts = { for key, _ in local.functions : key => "${trim(substr("${var.name}-${key}", 0, 23), "-")}-${local.hash}" }
  role_ids         = { for key, _ in local.functions : key => replace("${var.name}_${key}_${local.hash}", "-", "_") }
  # Bucket names are globally unique.
  bucket_name = "plrl-fn-${substr(sha1("${local.project_id}/${var.name}"), 0, 16)}"
}
