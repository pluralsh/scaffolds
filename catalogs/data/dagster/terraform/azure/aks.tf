resource "azurerm_kubernetes_cluster" "aks" {
  resource_group_name = var.resource_group_name
  name                = local.cluster_name
  location            = local.region
}

resource "kubernetes_service_account" "service_account" {
  metadata {
    name      = "dagster"
    namespace = "dagster"
    annotations = {
      "azure.workload.identity/client-id" = azurerm_user_assigned_identity.dagster.client_id
    }
  }
}