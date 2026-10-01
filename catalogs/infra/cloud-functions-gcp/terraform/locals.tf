locals {
  # Every function that can be deployed, by key. Each is defined in its own <key>.tf file with
  # what it needs: entry point, tool description and schema, environment and permissions.
  catalog = {
    volume-delete = local.volume_delete
    vm-delete     = local.vm_delete
  }

  functions = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown   = setsubtract(var.functions, keys(local.catalog))

  ctx_mgmt   = jsondecode(data.plural_service_context.mgmt.configuration)
  project_id = local.ctx_mgmt.project_id

  # Service account IDs are limited to 30 characters and custom role IDs to letters, digits,
  # underscores and dots, so both get a suffix hashed from the project and installation name.
  hash = substr(sha1("${local.project_id}/${var.name}"), 0, 6)
}
