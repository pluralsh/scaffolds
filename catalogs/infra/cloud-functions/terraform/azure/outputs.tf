output "functions" {
  description = "Deployed functions by key."
  value = {
    for key, app in azurerm_function_app_flex_consumption.function : key => {
      app        = app.name
      identifier = "${app.id}/functions/${local.functions[key].function}"
    }
  }

  precondition {
    condition     = length(local.unknown) == 0
    error_message = "Unknown functions: ${join(", ", local.unknown)}. Available: ${join(", ", keys(local.catalog))}."
  }
}

output "invoke_role_definition_id" {
  description = "Role to assign to the cloud connection service principal on the resource group so workbenches can invoke the functions."
  value       = azurerm_role_definition.invoke.role_definition_resource_id
}

output "workbench_tool_ids" {
  description = "Workbench tool IDs by function key, to attach to a workbench."
  value       = { for key, tool in plural_workbench_tool.function : key => tool.id }
}
