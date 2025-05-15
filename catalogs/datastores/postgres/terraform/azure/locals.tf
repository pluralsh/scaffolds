locals {
  identity_context = jsondecode(data.plural_service_context.identity.configuration)
  cluster_context = jsondecode(data.plural_service_context.mgmt.configuration)
  resource_group_name = local.cluster_context.resource_group_name
}
