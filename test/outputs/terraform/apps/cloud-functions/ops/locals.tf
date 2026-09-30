locals {
  # Every function that can be deployed. `statements` is the minimal IAM the function needs
  # on top of writing its own logs.
  catalog = {
    echo = {
      binary      = "echo-aws"
      description = "Echoes its input together with the IAM identity the function runs as. Changes nothing; used to verify the deployment."
      memory      = 128
      timeout     = 10
      # sts:GetCallerIdentity needs no permissions.
      statements = []
      schema     = jsonencode(jsondecode(file("${path.module}/schemas/echo.json")))
    }
  }

  functions      = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown        = setsubtract(var.functions, keys(local.catalog))
  function_names = { for key, _ in local.functions : key => "${var.name}-${key}" }
  artifacts      = { for key, fn in local.functions : key => "${var.artifact_dir}/${var.artifact_version}/${fn.binary}.zip" }
}
