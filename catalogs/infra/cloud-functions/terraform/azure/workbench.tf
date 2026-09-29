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

# The provider does not support `approval` yet, so every tool is invokable without human
# approval. Only register functions that change nothing until that is supported.
# TODO(PROD-5251): add `approval` to plural_workbench_tool in terraform-provider-plural and
# set it here for every function that changes resources.
# TODO(PROD-5251): fix the plural_cloud_connection data source (cloud_provider and
# configuration are required, so it cannot look connections up by name) and accept a
# connection name instead of cloud_connection_id.
resource "plural_workbench_tool" "function" {
  for_each = local.functions

  name                = replace("${var.name}-${each.key}", "-", "_")
  tool                = "AZURE_FUNCTION"
  cloud_connection_id = var.cloud_connection_id

  configuration = {
    azure_function = {
      identifier   = "${azurerm_function_app_flex_consumption.function[each.key].id}/functions/${each.value.function}"
      description  = each.value.description
      input_schema = each.value.schema
    }
  }
}
