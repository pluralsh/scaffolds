# Create Kubernetes service account for Dagster
resource "kubernetes_service_account" "dagster" {
  metadata {
    name      = local.service_account_name
    namespace = local.service_account_namespace
  }
}

