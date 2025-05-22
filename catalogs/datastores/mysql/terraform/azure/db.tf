resource "random_password" "db_password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

data "azurerm_resource_group" "default" {
  name = local.resource_group_name
}

resource "azurerm_mysql_flexible_server" "mysql" {
  name                          = var.name
  resource_group_name           = data.azurerm_resource_group.default.name
  location                      = data.azurerm_resource_group.default.location
  delegated_subnet_id           = local.network_context["mysql_subnet_id"]
  private_dns_zone_id           = local.network_context["mysql_dns_zone_id"]
  administrator_login           = var.db_username
  administrator_password        = random_password.db_password.result

  sku_name   = var.db_sku
  version    = var.db_version

  high_availability {
    mode = "ZoneRedundant"
  }

  storage {
    auto_grow_enabled = true
    size_gb = var.db_size_gb
  }

  lifecycle {
    ignore_changes = [zone, high_availability.0.standby_availability_zone]
  }
}

resource "azurerm_mysql_flexible_database" "mysql" {
  name                = var.name
  resource_group_name = data.azurerm_resource_group.default.name
  server_name         = azurerm_mysql_flexible_server.mysql.name
  charset             = "utf8"
  collation           = "utf8_unicode_ci"
}