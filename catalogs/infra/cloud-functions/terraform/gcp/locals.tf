locals {
  # Without allow_skip_snapshot, the `snapshot` input is dropped from the tool schema and the
  # function also rejects `snapshot: false`.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  # Deployable functions. `entry_point` is the function registered with the Functions
  # Framework in the shared Go source package. `permissions` is the minimal IAM each needs,
  # granted to its service account through a project custom role. All are registered as
  # workbench tools; calls to `destructive` ones (which change or delete resources) need human
  # approval.
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
    vm-delete = {
      entry_point = "VMDelete"
      description = "Deletes a standalone Compute Engine instance with its boot disk; data disks are detached and kept, and local SSDs are lost. Instances created by a managed instance group, GKE nodes and instances with deletion protection are refused. Use action plan first to see what is deleted and kept, then execute; execute again if it reports that the instance is still updating its disks."
      memory      = "512Mi"
      timeout     = 30
      destructive = true
      environment = {
        GOOGLE_CLOUD_PROJECT = local.project_id
      }
      # The function checks managed instance group and GKE membership itself; IAM can't
      # express those refusals. Changing a disk's auto-delete flag needs compute.disks.update.
      permissions = [
        "compute.instances.get",
        "compute.instances.delete",
        "compute.instances.setDiskAutoDelete",
        "compute.disks.update",
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/vm-delete.json")))
    }
  }

  ctx_mgmt   = jsondecode(data.plural_service_context.mgmt.configuration)
  project_id = local.ctx_mgmt.project_id

  functions     = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown       = setsubtract(var.functions, keys(local.catalog))
  service_names = { for key, _ in local.functions : key => "${var.name}-${key}" }
  # Every function is an entry point of the same Go source package.
  artifact = "${var.artifact_dir}/${var.artifact_version}/functions-gcp.zip"

  # Service account IDs are limited to 30 characters and custom role IDs to letters, digits,
  # underscores and dots, so both get a suffix hashed from the project and installation name.
  hash             = substr(sha1("${local.project_id}/${var.name}"), 0, 6)
  service_accounts = { for key, _ in local.functions : key => "${trim(substr("${var.name}-${key}", 0, 23), "-")}-${local.hash}" }
  # The build service account is shared by all functions. The name is capped at 17 characters
  # so the ID, with "-build-" and the hash, stays within 30.
  build_service_account = "${trim(substr(var.name, 0, 17), "-")}-build-${local.hash}"
  role_ids              = { for key, _ in local.functions : key => replace("${var.name}_${key}_${local.hash}", "-", "_") }
  # Name of the Cloud Run service behind each function: the last segment of
  # projects/<project>/locations/<region>/services/<name>.
  run_services = {
    for key, fn in google_cloudfunctions2_function.function : key => element(split("/", fn.service_config[0].service), length(split("/", fn.service_config[0].service)) - 1)
  }
  # Artifact Registry repository IDs are unique per project and region.
  repository_id = "${var.name}-functions"
  # Bucket names are globally unique.
  bucket_name = "plrl-fn-${substr(sha1("${local.project_id}/${var.name}"), 0, 16)}"
}
