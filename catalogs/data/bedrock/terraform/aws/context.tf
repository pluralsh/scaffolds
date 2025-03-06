data "plural_service_context" "mgmt" {
  name = "plrl/clusters/${var.cluster}"
}

locals {
  configuration    = jsondecode(data.plural_service_context.mgmt.configuration)
  eks_cluster_name = lookup(local.configuration, "cluster_name", "default-cluster")
  oidc_issuer_url  = replace(lookup(local.configuration, "eks_cluster_oidc_issuer_url", ""), "https://", "")
}

output "eks_cluster_name_debug" {
  value = local.eks_cluster_name
}

output "oidc_issuer_url_debug" {
  value = local.oidc_issuer_url
}

output "vpc_id_debug" {
  value = local.configuration["vpc_id"]
}

output "region_debug" {
  value = local.configuration["region"]
}
