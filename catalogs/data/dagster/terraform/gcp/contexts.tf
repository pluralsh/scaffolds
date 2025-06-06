data "plural_cluster" "cluster" {
  handle = var.cluster_handle
}

locals {
  tags = data.plural_cluster.cluster.tags
  tier = try(lookup(local.tags, "tier", "dev"), "dev")
}

data "plural_service_context" "network" {
  name = "plrl/vpc/${local.tier}"
}

data "plural_service_context" "cluster" {
  name = "plrl/clusters/mgmt"
}

locals {
  network_context = jsondecode(data.plural_service_context.network.configuration)
  cluster_context = jsondecode(data.plural_service_context.cluster.configuration)
  project_id   = local.cluster_context.project_id
  region       = local.cluster_context.region
  cluster_name = local.cluster_context.cluster_name
  network      = var.cluster_handle == "mgmt" ? local.cluster_context.network : local.network_context.network
}