locals {
  configuration = jsondecode(data.plural_service_context.cluster.configuration)
}

resource "random_password" "db_password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

data "azurerm_resource_group" "group" {
  name = var.resource_group_name
}

data "azurerm_storage_account" "airbyte" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.group.name
}

resource "azurerm_private_dns_zone" "postgres" {
  name                = var.db_dns_zone
  resource_group_name = data.azurerm_resource_group.group.name
}

resource "azurerm_private_dns_zone_virtual_network_link" "postgres" {
  name                  = var.network_link_name
  private_dns_zone_name = azurerm_private_dns_zone.postgres[0].name
  virtual_network_id    = azurerm_virtual_network.network.id
  resource_group_name   = data.azurerm_resource_group.group.name
}


resource "azurerm_postgresql_flexible_server" "postgres" {
  name                   = var.db_name
  resource_group_name    = var.resource_group_name
  location               = data.azurerm_resource_group.group.location
  version                = "13"
  delegated_subnet_id    = azurerm_subnet.postgres.id
  private_dns_zone_id    = azurerm_private_dns_zone.postgres[0].id
  administrator_login    = "console"
  administrator_password = random_password.db_password.result
  public_network_access_enabled = false

  storage_mb = var.db_disk
  sku_name   = var.db_sku

  high_availability {
    mode = "ZoneRedundant"
  }

  lifecycle {
    ignore_changes = [ zone, high_availability.0.standby_availability_zone ]
  }

  depends_on = [ azurerm_subnet.postgres ]
}

resource "azurerm_postgresql_flexible_server_database" "console" {
  name      = var.db_name
  server_id = azurerm_postgresql_flexible_server.postgres[0].id
  collation = "en_US.utf8"
  charset   = "utf8"
}