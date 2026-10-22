data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name              = local.cluster_context.cluster_name
  resource_group_name       = local.cluster_context.resource_group_name
  service_accounts          = toset(["metrics", "logs", "traces"])
  service_account_namespace = "plrl-telemetry"
}

data "azurerm_resource_group" "cluster" {
  name = local.resource_group_name
}

# The storage account is shared and pre-existing, so its blob versioning and
# lifecycle settings are left to its owner; only the container is managed here.
data "azurerm_storage_account" "telemetry" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.cluster.name
}

resource "azurerm_storage_container" "telemetry" {
  name                  = var.container_name
  storage_account_id    = data.azurerm_storage_account.telemetry.id
  container_access_type = "private"
}
