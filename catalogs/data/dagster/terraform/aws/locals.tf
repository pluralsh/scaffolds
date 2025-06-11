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
  subnet_ids                = local.network_context.subnet_ids
  vpc_id                    = local.network_context.vpc_id
  vpc_cidr                  = local.network_context.vpc_cidr
  db_name                   = "${var.db_name}-${local.cluster_name}"
  service_account_namespace = "dagster"
  service_account_name      = "dagster"
}