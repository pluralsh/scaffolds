locals {
  # Every function that can be deployed. `function` is the function name inside the app
  # (the folder holding function.json in the package) and `actions` is the minimal set of
  # ARM actions the function needs, granted to its managed identity on the resource group.
  catalog = {
    echo = {
      binary      = "echo-azure"
      function    = "echo"
      description = "Echoes its input together with the managed identity the function runs as. Changes nothing; used to verify the deployment."
      # Getting an ARM token needs no role assignments.
      actions = []
      schema  = jsonencode(jsondecode(file("${path.module}/schemas/echo.json")))
    }
  }

  identity_context    = jsondecode(data.plural_service_context.identity.configuration)
  cluster_context     = jsondecode(data.plural_service_context.cluster.configuration)
  resource_group_name = coalesce(var.resource_group_name, local.cluster_context.resource_group_name)

  functions    = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown      = setsubtract(var.functions, keys(local.catalog))
  artifact_key = { for key, fn in local.functions : key => "functions/${var.artifact_version}/${fn.binary}.zip" }

  # Function app and storage account names are globally unique, so they get a suffix derived
  # from the subscription, resource group and installation name. App names are limited to 32
  # characters and storage account names to 24 lowercase letters and digits.
  hash                 = sha1("${local.identity_context["subscription_id"]}/${local.resource_group_name}/${var.name}")
  app_names            = { for key, _ in local.functions : key => "${trim(substr("${var.name}-${key}", 0, 25), "-")}-${substr(local.hash, 0, 6)}" }
  storage_account_name = "plrlfn${substr(local.hash, 0, 18)}"
}
