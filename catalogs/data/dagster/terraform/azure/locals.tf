data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_name}"
}

data "plural_service_context" "network" {
  name = "plrl/network/${var.tier}"
}

data "azurerm_kubernetes_cluster" "cluster" {
  name = local.cluster_name
  resource_group_name = var.resource_group_name
}

locals {
  configuration    = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name = lookup(local.configuration, "cluster_name")
  network_configuration = jsondecode(data.plural_service_context.network.configuration)
}