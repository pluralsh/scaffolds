output "postgres_host" {
  value = azurerm_postgresql_flexible_server.postgres.fqdn
}

output "postgres_password" {
  value = random_password.db_password.result
  sensitive = true
}

output "encryption_key" {
  value = random_password.encryption_key.result
  sensitive = true
}
