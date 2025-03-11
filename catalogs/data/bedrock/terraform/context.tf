data "plural_service_context" "mgmt" {
  name = "plrl/clusters/${var.cluster}"
}

locals {
  configuration    = jsondecode(data.plural_service_context.mgmt.configuration)
  eks_cluster_name = lookup(local.configuration, "cluster_name")
}
