data "azurerm_resource_group" "cluster" {
  name = local.resource_group_name
}

data "azurerm_storage_account" "kestra" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.cluster.name
}

resource "azurerm_storage_container" "kestra" {
  name                  = var.storage_bucket
  storage_account_id    = data.azurerm_storage_account.kestra.id
  container_access_type = "private"

  lifecycle {
    precondition {
      condition     = !data.azurerm_storage_account.kestra.is_hns_enabled
      error_message = "Kestra internal storage requires an Azure Storage account with hierarchical namespace disabled."
    }
  }
}
