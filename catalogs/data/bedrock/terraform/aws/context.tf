data "aws_eks_cluster" "eks" {
  name = var.cluster
}

data "aws_vpc" "selected" {
  default = true
}

data "aws_subnets" "private" {
  filter {
    name   = "tag:Name"
    values = ["*private*"]
  }
}

data "aws_subnets" "public" {
  filter {
    name   = "tag:Name"
    values = ["*public*"]
  }
}

resource "plural_service_context" "mgmt" {
  name = "plrl/clusters/${var.cluster}"

  configuration = jsonencode({
    region          = var.region
    cluster_name    = data.aws_eks_cluster.eks.name
    vpc_id          = data.aws_vpc.selected.id
    subnet_ids      = concat(data.aws_subnets.public.ids, data.aws_subnets.private.ids)
    private_subnets = data.aws_subnets.private.ids
    public_subnets  = data.aws_subnets.public.ids
    vpc_cidr        = data.aws_vpc.selected.cidr_block
    eks_cluster_oidc_issuer_url = data.aws_eks_cluster.eks.identity[0].oidc[0].issuer  
  })
}

data "plural_service_context" "mgmt" {
  name = "plrl/clusters/mgmt"
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
