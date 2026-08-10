resource "azurerm_postgresql_flexible_server" "kestra" {
  name                          = "${local.resource_name}-${random_string.database_suffix.result}"
  resource_group_name           = data.azurerm_resource_group.cluster.name
  location                      = data.azurerm_resource_group.cluster.location
  version                       = var.postgres_version
  delegated_subnet_id           = local.pg_subnet_id
  private_dns_zone_id           = local.dns_zone_id
  administrator_login           = "kestra"
  administrator_password        = random_password.database.result
  public_network_access_enabled = false

  storage_mb                   = var.db_storage_mb
  auto_grow_enabled            = true
  sku_name                     = var.db_sku
  backup_retention_days        = var.backup_retention_days
  geo_redundant_backup_enabled = false

  high_availability {
    mode = "ZoneRedundant"
  }

  lifecycle {
    ignore_changes = [zone, high_availability[0].standby_availability_zone]
  }

  tags = local.tags
}

resource "azurerm_postgresql_flexible_server_database" "kestra" {
  name      = "kestra"
  server_id = azurerm_postgresql_flexible_server.kestra.id
  collation = "en_US.utf8"
  charset   = "utf8"
}
