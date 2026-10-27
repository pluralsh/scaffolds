data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context           = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name              = local.cluster_context.cluster_name
  resource_group_name       = local.cluster_context.resource_group_name
  service_accounts          = toset(["metrics", "logs", "traces"])
  service_account_namespace = "plrl-telemetry"

  # Storage account names are unique across all of Azure, 3-24 lowercase letters and
  # digits, while cluster names are only unique per resource group, so the hash
  # includes the subscription and resource group. Do not change this recipe on a live
  # install: it would replace the account and delete the stored telemetry.
  storage_account_name = "plrltel${substr(md5("${local.identity_context.subscription_id}/${local.resource_group_name}/${local.cluster_name}"), 0, 12)}"
}

data "azurerm_resource_group" "cluster" {
  name = local.resource_group_name
}

# A dedicated account, so nothing else shares its settings or data.
resource "azurerm_storage_account" "telemetry" {
  name                = local.storage_account_name
  resource_group_name = data.azurerm_resource_group.cluster.name
  location            = data.azurerm_resource_group.cluster.location

  account_kind             = "StorageV2"
  account_tier             = "Standard"
  account_replication_type = "LRS"

  min_tls_version                 = "TLS1_2"
  allow_nested_items_to_be_public = false

  # No cross-tenant object replication, which defaults to on before azurerm 4.0.
  cross_tenant_replication_enabled = false
}

resource "azurerm_storage_container" "telemetry" {
  name                  = var.container_name
  storage_account_id    = azurerm_storage_account.telemetry.id
  container_access_type = "private"
}
