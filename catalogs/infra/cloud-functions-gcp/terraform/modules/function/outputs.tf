output "service" {
  description = "Name of the Cloud Run service behind the function."
  value       = local.run_service
}

output "uri" {
  description = "URL the function is invoked at."
  value       = google_cloudfunctions2_function.function.service_config[0].uri
}

output "workbench_tool_id" {
  description = "ID of the function's workbench tool."
  value       = plural_workbench_tool.function.id
}

output "service_account_email" {
  description = "Email of the service account the function runs as."
  value       = google_service_account.function.email
}
