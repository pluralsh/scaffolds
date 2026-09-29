output "functions" {
  description = "Deployed functions by key."
  value = {
    for key, svc in google_cloud_run_v2_service.function : key => {
      service = svc.name
      uri     = svc.uri
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
