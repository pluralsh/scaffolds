# Allows resolving the function URLs and keys, which is how workbenches invoke Azure
# functions, and nothing else. Assign it to the service principal of the workbench cloud
# connection on the resource group.
resource "azurerm_role_definition" "invoke" {
  name              = "${var.name}-invoke-${substr(local.hash, 0, 6)}"
  scope             = data.azurerm_resource_group.functions.id
  description       = "Invoke the ${var.name} operational functions from Plural workbenches."
  assignable_scopes = [data.azurerm_resource_group.functions.id]

  permissions {
    actions = [
      "Microsoft.Web/sites/functions/read",
      "Microsoft.Web/sites/functions/listsecrets/action",
      "Microsoft.Web/sites/host/listkeys/action",
    ]
  }
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

# Tools of functions that change resources require human approval of every call.
resource "plural_workbench_tool" "function" {
  for_each = local.tools

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
