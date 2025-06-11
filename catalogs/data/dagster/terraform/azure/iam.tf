resource "azurerm_user_assigned_identity" "dagster" {
  name                = "${local.cluster_name}-dagster"
  resource_group_name = data.azurerm_resource_group.resource_group.name
  location            = data.azurerm_resource_group.resource_group.location
}

resource "azurerm_role_assignment" "dagster" {
  scope                = data.azurerm_resource_group.resource_group.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.dagster.principal_id
}

resource "azurerm_federated_identity_credential" "dagster" {
  name                = "${local.cluster_name}-dagster"
  resource_group_name = data.azurerm_resource_group.resource_group.name
  audience = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.aks.oidc_issuer_url
  parent_id           = azurerm_user_assigned_identity.dagster.id
  subject             = "system:serviceaccount:${local.service_account_name}:${local.service_account_namespace}"
}