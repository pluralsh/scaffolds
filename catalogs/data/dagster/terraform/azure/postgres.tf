resource "random_password" "db_password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "azurerm_postgresql_flexible_server" "postgres" {
  name                          = local.db_name
  resource_group_name           = data.azurerm_resource_group.resource_group.name
  location                      = data.azurerm_resource_group.resource_group.location
  version                       = var.postgres_vsn
  delegated_subnet_id           = local.pg_subnet_id
  private_dns_zone_id           = local.dns_zone_id
  administrator_login           = "dagster"
  administrator_password        = random_password.db_password.result
  public_network_access_enabled = false

  storage_mb = var.db_disk
  sku_name   = var.db_sku

  high_availability {
    mode = "ZoneRedundant"
  }

  lifecycle {
    ignore_changes = [zone, high_availability.0.standby_availability_zone]
  }
}

resource "azurerm_postgresql_flexible_server_configuration" "default" {
  name      = "azure.extensions"
  server_id = azurerm_postgresql_flexible_server.postgres.id
  value     = "BTREE_GIN"
}

resource "azurerm_postgresql_flexible_server_database" "postgres" {
  name      = "dagster"
  server_id = azurerm_postgresql_flexible_server.postgres.id
  collation = "en_US.utf8"
  charset   = "utf8"
}