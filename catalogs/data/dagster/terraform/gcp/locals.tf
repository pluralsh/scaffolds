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
  project_id    = local.mgmt_context.project_id
  region        = local.mgmt_context.region
  cluster_name  = local.mgmt_context.cluster_name
  network       = local.network_context.network
  network_short = split("/", local.network)[length(split("/", local.network)) - 1]
  db_name = "${var.db_name}-${local.cluster_name}"
}