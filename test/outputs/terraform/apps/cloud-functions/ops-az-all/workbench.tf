# Allows resolving a function's URL and its own key, which is how workbenches invoke Azure
# functions, and nothing else. Host keys are left out on purpose: they include the app's
# master key, which allows calling any function of the app (e.g. ssh-access's timer) and
# managing its keys; cloud-query only falls back to them when a function has no key of its
# own. Assign it to the service principal of the workbench cloud connection on each of
# invoke_scopes.
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
