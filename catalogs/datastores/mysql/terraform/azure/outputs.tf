output "db_host" {
  value = azurerm_mysql_flexible_server.mysql.fqdn
}

output "db_user" {
  value = azurerm_mysql_flexible_server.mysql.administrator_login
  sensitive = true
}

output "db_password" {
  value = azurerm_mysql_flexible_server.mysql.administrator_password
  sensitive = true
}
