output "db_host" {
  value = azurerm_postgresql_flexible_server.postgres.fqdn
}

output "db_user" {
  value = azurerm_postgresql_flexible_server.postgres.administrator_login
  sensitive = true
}

output "db_password" {
  value = azurerm_postgresql_flexible_server.postgres.administrator_password
  sensitive = true
}
