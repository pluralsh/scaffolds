data "azurerm_kubernetes_cluster" "cluster" {
  name                = local.cluster_name
  resource_group_name = local.resource_group_name
}

resource "azurerm_user_assigned_identity" "tempo" {
  name                = "${local.cluster_name}-tempo"
  resource_group_name = data.azurerm_resource_group.cluster.name
  location            = data.azurerm_resource_group.cluster.location
}

resource "azurerm_role_assignment" "tempo" {
  scope                = data.azurerm_storage_account.tempo.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.tempo.principal_id
}

resource "azurerm_federated_identity_credential" "tempo" {
  name                = "${local.cluster_name}-tempo"
  resource_group_name = data.azurerm_resource_group.cluster.name
  audience            = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.cluster.oidc_issuer_url
  parent_id           = azurerm_user_assigned_identity.tempo.id
  subject             = "system:serviceaccount:${local.service_account_namespace}:${local.service_account_name}"
}
