data "azurerm_kubernetes_cluster" "cluster" {
  name                = local.cluster_name
  resource_group_name = local.resource_group_name
}

resource "azurerm_user_assigned_identity" "telemetry" {
  name                = "${local.cluster_name}-telemetry"
  resource_group_name = data.azurerm_resource_group.cluster.name
  location            = data.azurerm_resource_group.cluster.location
}

resource "azurerm_role_assignment" "telemetry" {
  scope                = azurerm_storage_container.telemetry.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.telemetry.principal_id
}

# One federated credential per product service account, all on the same identity.
resource "azurerm_federated_identity_credential" "telemetry" {
  for_each = local.service_accounts

  name                = "${local.cluster_name}-telemetry-${each.key}"
  resource_group_name = data.azurerm_resource_group.cluster.name
  audience            = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.cluster.oidc_issuer_url
  parent_id           = azurerm_user_assigned_identity.telemetry.id
  subject             = "system:serviceaccount:${local.service_account_namespace}:${each.key}"
}
