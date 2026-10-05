locals {
  # Every function that can be deployed, by key. Each is defined in its own <key>.tf file with
  # what it needs: entry point, tool description and schema, environment, permissions and the APIs
  # it calls.
  catalog = {
    volume-delete      = local.volume_delete
    vm-delete          = local.vm_delete
    node-pool-resize   = local.node_pool_resize
    lb-frontend-delete = local.lb_frontend_delete
    db-restore         = local.db_restore
    ssh-access         = local.ssh_access
  }

  functions = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown   = setsubtract(var.functions, keys(local.catalog))

  project_id = jsondecode(data.plural_service_context.cluster.configuration).project_id

  # Service account and custom role IDs are limited in length and characters, so they get a
  # suffix hashed from the project and installation name, and for a function's, its key.
  hash          = substr(sha1("${local.project_id}/${var.name}"), 0, 6)
  function_hash = { for key in keys(local.catalog) : key => substr(sha1("${local.project_id}/${var.name}/${key}"), 0, 6) }
}
