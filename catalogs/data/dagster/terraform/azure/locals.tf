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
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  network_context = jsondecode(data.plural_service_context.network.configuration)
  cluster_context = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name              = local.cluster_context.cluster_name
  region                    = local.cluster_context.region
  resource_group_name       = local.cluster_context.resource_group_name
  pg_subnet_id              = local.network_context.pg_subnet_id
  dns_zone_id               = local.network_context.dns_zone_id
  db_name                   = "${var.db_name}-${local.cluster_name}"
  service_account_namespace = "dagster"
  service_account_name      = "dagster"
}