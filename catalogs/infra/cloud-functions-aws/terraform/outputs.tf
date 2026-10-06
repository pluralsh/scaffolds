output "functions" {
  description = "Deployed functions by key."
  value = {
    for key, fn in aws_lambda_function.function : key => {
      name = fn.function_name
      arn  = fn.arn
    }
  }

  precondition {
    condition     = length(local.unknown) == 0
    error_message = "Unknown functions: ${join(", ", local.unknown)}. Available: ${join(", ", keys(local.catalog))}."
  }
}

output "invoke_policy_arn" {
  description = "IAM policy to attach to the cloud connection principal so workbenches can invoke the functions."
  value       = aws_iam_policy.invoke.arn
}

output "workbench_tool_ids" {
  description = "Workbench tool IDs by function key, to attach to a workbench."
  value       = { for key, tool in plural_workbench_tool.function : key => tool.id }
}
