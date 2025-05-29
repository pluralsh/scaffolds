locals {
  identity_context = jsondecode(data.plural_service_context.identity.configuration)
  network_context = jsondecode(data.plural_service_context.network.configuration)
  cluster_context = jsondecode(data.plural_service_context.cluster.configuration)
  resource_group_name = local.cluster_context.resource_group_name
}


