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

data "azurerm_resource_group" "default" {
  name = var.resource_group_name
}

data "azurerm_storage_account" "airbyte" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.default.name
}

resource "azurerm_virtual_network" "default" {
  name                = var.network_name
  address_space       = var.network_cidrs
  location            = data.azurerm_resource_group.default.location
  resource_group_name = data.azurerm_resource_group.default.name
}


resource "azurerm_subnet" "postgres" {
  name                 = "${var.network_name}-postgres"
  resource_group_name  = data.azurerm_resource_group.default.name
  virtual_network_name = azurerm_virtual_network.default.name
  address_prefixes     = var.postgres_cidrs
  service_endpoints    = ["Microsoft.Storage"]
  delegation {
    name = "fs"
    service_delegation {
      name = "Microsoft.DBforPostgreSQL/flexibleServers"
      actions = [
        "Microsoft.Network/virtualNetworks/subnets/join/action",
      ]
    }
  }
}

resource "azurerm_private_dns_zone" "postgres" {
  name                = var.db_dns_zone
  resource_group_name = data.azurerm_resource_group.default.name
}

resource "azurerm_private_dns_zone_virtual_network_link" "postgres" {
  name                  = var.network_link_name
  private_dns_zone_name = azurerm_private_dns_zone.postgres.name
  virtual_network_id    = azurerm_virtual_network.default.id
  resource_group_name   = data.azurerm_resource_group.default.name
}

resource "azurerm_postgresql_flexible_server" "postgres" {
  name                   = var.db_name
  resource_group_name    = data.azurerm_resource_group.default.name
  location               = data.azurerm_resource_group.default.location
  version                = "13"
  delegated_subnet_id    = azurerm_subnet.postgres.id
  private_dns_zone_id    = azurerm_private_dns_zone.postgres.id
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

resource "azurerm_postgresql_flexible_server_database" "postgres" {
  name      = var.db_name
  server_id = azurerm_postgresql_flexible_server.postgres.id
  collation = "en_US.utf8"
  charset   = "utf8"
}