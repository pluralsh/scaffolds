data "plural_service_context" "dev_network" {
  name = "plrl/network/dev"
}

locals {
  configuration = jsondecode(data.plural_service_context.dev_network.configuration)
}

resource "random_password" "db_password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

data "azurerm_resource_group" "default" {
  name = var.resource_group_name
}

data "azurerm_storage_account" "airbyte" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.default.name
}

resource "azurerm_postgresql_flexible_server" "postgres" {
  name                   = var.db_name
  resource_group_name    = data.azurerm_resource_group.default.name
  location               = data.azurerm_resource_group.default.location
  version                = "13"
  delegated_subnet_id    = local.configuration["pg_subnet_id"]
  private_dns_zone_id    = local.configuration["dns_zone_id"]
  administrator_login    = "airbyte"
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
}

resource "azurerm_postgresql_flexible_server_configuration" "default" {
  name      = "azure.extensions"
  server_id = azurerm_postgresql_flexible_server.postgres.id
  value     = "BTREE_GIN"
}

resource "azurerm_postgresql_flexible_server_database" "postgres" {
  name      = "airbyte"
  server_id = azurerm_postgresql_flexible_server.postgres.id
  collation = "en_US.utf8"
  charset   = "utf8"
}