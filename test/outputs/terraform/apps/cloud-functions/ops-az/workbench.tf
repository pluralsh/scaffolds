# Function URLs and keys only. Host keys are left out: they include the master key.
resource "azurerm_role_definition" "invoke" {
  name              = "${var.name}-invoke-${substr(local.hash, 0, 6)}"
  scope             = data.azurerm_resource_group.functions.id
  description       = "Invoke the ${var.name} operational functions from Plural workbenches."
  assignable_scopes = [data.azurerm_resource_group.functions.id]

  permissions {
    actions = [
      "Microsoft.Web/sites/functions/read",
      "Microsoft.Web/sites/functions/listsecrets/action",
    ]
  }
}

resource "azurerm_role_assignment" "invoke" {
  for_each = var.invoker_principal_id == null ? {} : local.functions

  scope              = azurerm_function_app_flex_consumption.function[each.key].id
  role_definition_id = azurerm_role_definition.invoke.role_definition_resource_id
  principal_id       = var.invoker_principal_id
  principal_type     = "ServicePrincipal"
}

data "plural_cloud_connection" "workbench" {
  name = var.cloud_connection

  lifecycle {
    postcondition {
      condition     = self.cloud_provider == "AZURE"
      error_message = "Cloud connection ${var.cloud_connection} is for ${self.cloud_provider}, not AZURE."
    }
  }
}

resource "plural_workbench_tool" "function" {
  for_each = local.functions

  name                = replace("${var.name}-${each.key}", "-", "_")
  tool                = "AZURE_FUNCTION"
  cloud_connection_id = data.plural_cloud_connection.workbench.id
  approval            = each.value.destructive

  configuration = {
    azure_function = {
      identifier   = "${azurerm_function_app_flex_consumption.function[each.key].id}/functions/${each.value.function}"
      description  = each.value.description
      input_schema = each.value.schema
    }
  }
}
