data "azurerm_kubernetes_cluster" "cluster" {
  name                = local.cluster_name
  resource_group_name = var.resource_group_name
}

data "plural_cluster" "cluster" {
  handle = var.cluster_handle
}

locals {
  tags = data.plural_cluster.cluster.tags
  tier = var.cluster_handle == "mgmt" ? "plural" : try(lookup(local.tags, "tier", "dev"), "dev")
}

data "plural_service_context" "network" {
  name = "plrl/vpc/${local.tier}"
}

data "plural_service_context" "cluster" {
  name = "plrl/clusters/mgmt"
}

locals {
  network_context = jsondecode(data.plural_service_context.network.configuration)
  mgmt_context = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name = local.mgmt_context.cluster_name
  region       = local.mgmt_context.region
  pg_subnet_id = local.network_context.pg_subnet_id
  dns_zone_id  = local.network_context.dns_zone_id
  db_name      = "${var.db_name}-${local.cluster_name}"
}