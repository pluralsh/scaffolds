locals {
  # Without allow_skip_snapshot, the `snapshot` input is removed from the tool schema and the
  # function rejects `snapshot: false` as well.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  # Every function that can be deployed. `function` is the function name inside the app
  # (the folder holding function.json in the package), `actions` is the minimal set of ARM
  # actions the function needs and `scopes` the resource groups its managed identity gets
  # them on. `destructive` functions change or delete resources and are only registered as
  # workbench tools when register_destructive_tools is set.
  catalog = {
    echo = {
      binary      = "echo-azure"
      function    = "echo"
      description = "Echoes its input together with the managed identity the function runs as. Changes nothing; used to verify the deployment."
      destructive = false
      environment = {}
      # Getting an ARM token needs no role assignments.
      actions = []
      scopes  = []
      schema  = jsonencode(jsondecode(file("${path.module}/schemas/echo.json")))
    }
    volume-delete = {
      binary      = "volume-delete-azure"
      function    = "volume-delete"
      description = "Deletes an unattached managed disk that Kubernetes created for a PersistentVolume. Before calling it, confirm in the cluster that the PersistentVolume no longer exists and pass its name as pvName. Use action plan first; execute takes an incremental snapshot and keeps the disk, and a later execute deletes it once the snapshot has completed."
      destructive = true
      environment = { ALLOW_SKIP_SNAPSHOT = tostring(var.allow_skip_snapshot) }
      actions = [
        "Microsoft.Compute/disks/read",
        "Microsoft.Compute/disks/delete",
        # Creating a snapshot copies the source disk through a read access grant.
        "Microsoft.Compute/disks/beginGetAccess/action",
        "Microsoft.Compute/snapshots/read",
        "Microsoft.Compute/snapshots/write",
      ]
      scopes = var.volume_delete_scopes
      schema = jsonencode(merge(local.volume_delete_schema, { properties = local.volume_delete_properties }))
    }
  }

  identity_context    = jsondecode(data.plural_service_context.identity.configuration)
  cluster_context     = jsondecode(data.plural_service_context.cluster.configuration)
  resource_group_name = coalesce(var.resource_group_name, local.cluster_context.resource_group_name)

  functions = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  # Functions registered as workbench tools, and the only ones the cloud connection should be
  # allowed to invoke.
  tools   = { for key, fn in local.functions : key => fn if !fn.destructive || var.register_destructive_tools }
  unknown = setsubtract(var.functions, keys(local.catalog))
  # The version is part of the path, so a new release changes zip_deploy_file and redeploys.
  artifacts = { for key, fn in local.functions : key => "${var.artifact_dir}/${var.artifact_version}/${fn.binary}.zip" }

  # Function app and storage account names are globally unique, so they get a suffix derived
  # from the subscription, resource group and installation name. App names are limited to 32
  # characters and storage account names to 24 lowercase letters and digits.
  hash                 = sha1("${local.identity_context["subscription_id"]}/${local.resource_group_name}/${var.name}")
  app_names            = { for key, _ in local.functions : key => "${trim(substr("${var.name}-${key}", 0, 25), "-")}-${substr(local.hash, 0, 6)}" }
  storage_account_name = "plrlfn${substr(local.hash, 0, 18)}"
}
