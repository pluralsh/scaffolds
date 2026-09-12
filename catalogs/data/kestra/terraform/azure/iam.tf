data "azurerm_kubernetes_cluster" "cluster" {
  name                = local.cluster_name
  resource_group_name = local.resource_group_name
}

resource "azurerm_user_assigned_identity" "kestra" {
  name                = local.resource_name
  resource_group_name = data.azurerm_resource_group.cluster.name
  location            = data.azurerm_resource_group.cluster.location
  tags                = local.tags
}

resource "azurerm_role_assignment" "kestra_storage" {
  scope                = azurerm_storage_container.kestra.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.kestra.principal_id
}

resource "azurerm_federated_identity_credential" "kestra" {
  name                = local.resource_name
  resource_group_name = data.azurerm_resource_group.cluster.name
  audience            = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.cluster.oidc_issuer_url
  parent_id           = azurerm_user_assigned_identity.kestra.id
  subject             = "system:serviceaccount:${local.service_account_namespace}:${local.service_account_name}"
}
