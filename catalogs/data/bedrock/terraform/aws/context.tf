data "plural_service_context" "mgmt" {
  name = "plrl/clusters/${var.cluster}"
}

locals {
  configuration    = jsondecode(data.plural_service_context.mgmt.configuration)
  eks_cluster_name = lookup(local.configuration, "cluster_name", "default-cluster")
}

output "eks_cluster_name_debug" {
  value = local.eks_cluster_name
}

output "vpc_id_debug" {
  value = local.configuration["vpc_id"]
}

output "region_debug" {
  value = local.configuration["region"]
}
