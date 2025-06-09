data "azurerm_kubernetes_cluster" "aks" {
  resource_group_name = local.resource_group_name
  name                = local.cluster_name
}

resource "kubernetes_service_account" "service_account" {
  metadata {
    name      = local.service_account_name
    namespace = local.service_account_namespace
    annotations = {
      "azure.workload.identity/client-id" = azurerm_user_assigned_identity.dagster.client_id
    }
  }
}