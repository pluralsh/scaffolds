data "azurerm_resource_group" "resource_group" {
  name = var.resource_group_name
}

data "azurerm_storage_account" "sa" {
  name                = var.storage_account_name
  resource_group_name = data.azurerm_resource_group.resource_group.name
}

resource "azurerm_storage_container" "dagster" {
  name                  = var.dagster_bucket
  storage_account_id    = data.azurerm_storage_account.sa.id
  container_access_type = "private"
}

## Setting up User Assigned Identity for access to blob storage

resource "azurerm_user_assigned_identity" "dagster" {
  name                = "dagster-${local.cluster_name}"
  resource_group_name = data.azurerm_resource_group.resource_group.name
  location            = data.azurerm_resource_group.resource_group.location
}

resource "azurerm_role_assignment" "storage_blob_data_contributor" {
  scope                = azurerm_storage_container.dagster.resource_manager_id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.dagster.principal_id
}


resource "azurerm_federated_identity_credential" "dagster" {
  name                = "dagster-${local.cluster_name}"
  resource_group_name = data.azurerm_resource_group.resource_group.name
  audience = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.cluster.oidc_issuer_url
  parent_id           = azurerm_user_assigned_identity.dagster.id
  subject             = "system:serviceaccount:dagster:dagster"
}

resource "azurerm_federated_identity_credential" "user_deployments" {
  name                = "dagster-${local.cluster_name}-user-deployments"
  resource_group_name = data.azurerm_resource_group.resource_group.name
  audience = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.cluster.oidc_issuer_url
  parent_id           = azurerm_user_assigned_identity.dagster.id
  subject             = "system:serviceaccount:dagster:dagster-user-deployments"
}