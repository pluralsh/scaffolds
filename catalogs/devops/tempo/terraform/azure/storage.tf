data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name              = local.cluster_context.cluster_name
  resource_group_name       = local.cluster_context.resource_group_name
  service_account_name      = "tempo"
  service_account_namespace = "tempo"
}

data "azurerm_resource_group" "cluster" {
  name = local.resource_group_name
}

data "azurerm_storage_account" "tempo" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.cluster.name
}

resource "azurerm_storage_container" "tempo" {
  name                  = var.container_name
  storage_account_id    = data.azurerm_storage_account.tempo.id
  container_access_type = "private"
}
