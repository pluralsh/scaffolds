data "azurerm_resource_group" "resource_group" {
  name = local.resource_group_name
}

data "azurerm_storage_account" "sa" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.resource_group.name
}

resource "azurerm_storage_container" "dagster" {
  name                  = "${local.cluster_name}-dagster"
  storage_account_id    = data.azurerm_storage_account.sa.id
  container_access_type = "private"
}