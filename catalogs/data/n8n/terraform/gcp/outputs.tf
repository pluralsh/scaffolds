output "postgres_host" {
  value = module.pg.private_ip_address
}

output "postgres_password" {
  description = "The database password"
  value       = random_password.password.result
  sensitive   = true
}

output "encryption_key" {
  value = random_password.encryption_key.result
  sensitive = true
}
