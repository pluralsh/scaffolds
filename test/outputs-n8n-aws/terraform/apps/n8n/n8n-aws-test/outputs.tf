output "postgres_host" {
  value = try(module.db.db_instance_address, "")
}

output "postgres_password" {
  value = random_password.password.result
  sensitive = true
}

output "encryption_key" {
  value = random_password.encryption_key.result
  sensitive = true
}
