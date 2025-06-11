data "plural_cluster" "cluster" {
  handle = var.cluster_handle
}

locals {
  tags = data.plural_cluster.cluster.tags
  tier = var.cluster_handle == "mgmt" ? "plural" : try(lookup(local.tags, "tier", "dev"), "dev")
}

data "plural_service_context" "network" {
  name = "plrl/network/${local.tier}"
}

data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

data "plural_service_context" "identity" {
  name = "plrl/azure/identity"
}

locals {
  network_context = jsondecode(data.plural_service_context.network.configuration)
  cluster_context = jsondecode(data.plural_service_context.cluster.configuration)
  identity_context = jsondecode(data.plural_service_context.identity.configuration)

  # Cluster context variables
  cluster_name = local.cluster_context.cluster_name
  resource_group_name = local.cluster_context.resource_group_name

  # Database variables
  pg_subnet_id = local.network_context.pg_subnet_id
  dns_zone_id  = local.network_context.dns_zone_id
  db_name = "${var.db_name}-${local.cluster_name}"

  // Identity context variables
  subscription_id = local.identity_context.subscription_id
  client_id = local.identity_context.client_id
  tenant_id = local.identity_context.tenant_id

  # Service account variables
  service_account_namespace = "dagster"
  service_account_name      = "dagster"
}