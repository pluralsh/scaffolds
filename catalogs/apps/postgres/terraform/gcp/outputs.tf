output "private_ip_address" {
  value = module.pg.private_ip_address
}

output "public_ip_address" {
  value = module.pg.public_ip_address
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