output "functions" {
  description = "Deployed functions by key."
  value = {
    for key, fn in google_cloudfunctions2_function.function : key => {
      service = local.run_services[key]
      uri     = fn.service_config[0].uri
    }
  }

  precondition {
    condition     = length(local.unknown) == 0
    error_message = "Unknown functions: ${join(", ", local.unknown)}. Available: ${join(", ", keys(local.catalog))}."
  }
}

output "workbench_tool_ids" {
  description = "Workbench tool IDs by function key, to attach to a workbench."
  value       = { for key, tool in plural_workbench_tool.function : key => tool.id }
}
