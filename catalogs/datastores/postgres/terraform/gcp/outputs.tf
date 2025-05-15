output "db_pg" {
  value     = module.pg
  sensitive = true
}

output "db_name" {
  description = "The name of the database"
  value       = var.name
}

output "db_user" {
  description = "The database user"
  value       = var.user_name
}

output "db_password" {
  description = "The database password"
  value       = random_password.password.result
  sensitive   = true
}

output "db_url" {
  value     = local.db_url
  sensitive = true
}