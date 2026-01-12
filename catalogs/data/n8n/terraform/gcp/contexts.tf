data "plural_cluster" "cluster" {
  handle = var.cluster_name
}

locals {
  tags = data.plural_cluster.cluster.tags
  tier = var.cluster_name == "mgmt" ? "plural" : try(lookup(local.tags, "tier", "dev"), "dev")
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
  project_id = local.cluster_context.project_id
  region     = local.cluster_context.region
  network    = local.network_context.network
}

