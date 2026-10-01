output "functions" {
  description = "Deployed functions by key."
  value = {
    for key, fn in module.function : key => {
      service         = fn.service
      uri             = fn.uri
      service_account = fn.service_account_email
    }
  }

  precondition {
    condition     = length(local.unknown) == 0
    error_message = "Unknown functions: ${join(", ", local.unknown)}. Available: ${join(", ", keys(local.catalog))}."
  }
}

output "workbench_tool_ids" {
  description = "Workbench tool IDs by function key, to attach to a workbench."
  value       = { for key, fn in module.function : key => fn.workbench_tool_id }
}
