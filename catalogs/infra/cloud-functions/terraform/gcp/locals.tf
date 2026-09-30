locals {
  # Without allow_skip_snapshot, the `snapshot` input is removed from the tool schema and the
  # function rejects `snapshot: false` as well.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  # Every function that can be deployed. `entry_point` is the function registered with the
  # Functions Framework in the shared Go source package, and `permissions` is the minimal set
  # of IAM permissions the function needs, granted to its service account through a project
  # custom role.
  # `destructive` functions change or delete resources and are only registered as workbench
  # tools when register_destructive_tools is set,
  # and every call of their tools requires human approval.
  catalog = {
    volume-delete = {
      entry_point = "VolumeDelete"
      description = "Deletes an unattached zonal persistent disk that Kubernetes created for a PersistentVolume. Before calling it, confirm in the cluster that the PersistentVolume no longer exists and pass its name as pvName. Use action plan first; execute takes a snapshot and keeps the disk, and a later execute deletes it once the snapshot has completed."
      memory      = "512Mi"
      timeout     = 30
      destructive = true
      environment = {
        ALLOW_SKIP_SNAPSHOT  = tostring(var.allow_skip_snapshot)
        GOOGLE_CLOUD_PROJECT = local.project_id
      }
      permissions = [
        "compute.disks.get",
        "compute.disks.delete",
        "compute.disks.createSnapshot",
        "compute.snapshots.create",
        "compute.snapshots.list",
        "compute.snapshots.setLabels",
      ]
      schema = jsonencode(merge(local.volume_delete_schema, { properties = local.volume_delete_properties }))
    }
  }

  ctx_mgmt   = jsondecode(data.plural_service_context.mgmt.configuration)
  project_id = local.ctx_mgmt.project_id

  functions     = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown       = setsubtract(var.functions, keys(local.catalog))
  service_names = { for key, _ in local.functions : key => "${var.name}-${key}" }
  # Functions registered as workbench tools, and the only ones the cloud connection may invoke.
  tools = { for key, fn in local.functions : key => fn if !fn.destructive || var.register_destructive_tools }
  # Every function is an entry point of the same Go source package.
  artifact = "${var.artifact_dir}/${var.artifact_version}/functions-gcp.zip"

  # Service account IDs are limited to 30 characters and custom role IDs to letters, digits,
  # underscores and dots, so both get a suffix derived from the installation name.
  hash             = substr(sha1("${local.project_id}/${var.name}"), 0, 6)
  service_accounts = { for key, _ in local.functions : key => "${trim(substr("${var.name}-${key}", 0, 23), "-")}-${local.hash}" }
  # The build service account is shared by every function. The name is capped at 17
  # characters so that, with "-build-" and the hash, the ID stays within 30.
  build_service_account = "${trim(substr(var.name, 0, 17), "-")}-build-${local.hash}"
  role_ids              = { for key, _ in local.functions : key => replace("${var.name}_${key}_${local.hash}", "-", "_") }
  # The name of the Cloud Run service behind each function, the last segment of
  # projects/<project>/locations/<region>/services/<name>.
  run_services = {
    for key, fn in google_cloudfunctions2_function.function : key => element(split("/", fn.service_config[0].service), length(split("/", fn.service_config[0].service)) - 1)
  }
  # Bucket names are globally unique.
  bucket_name = "plrl-fn-${substr(sha1("${local.project_id}/${var.name}"), 0, 16)}"
}
