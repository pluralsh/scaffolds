data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name    = local.cluster_context.cluster_name
  subnet_ids      = try(local.cluster_context.private_subnets, local.cluster_context.subnet_ids)
  instance_types  = [for t in split(",", var.instance_types) : trimspace(t)]
  node_role_name  = substr("${local.cluster_name}-${var.pool}-nodes", 0, 64)

  labels = {
    "plural.sh/devbox-pool" = var.pool
  }
  taints = {
    sandbox = { key = "ate.dev/sandboxClass", value = var.sandbox_class }
    disk    = { key = "plural.sh/devbox-disk", value = "pending" }
  }
}

data "aws_eks_cluster" "cluster" {
  name = local.cluster_name
}

# The node security group of terraform-aws-modules/eks, which the cluster's other nodes (and CoreDNS) run in and
# which only accepts traffic from its own members. Clusters without one match nothing.
data "aws_security_groups" "cluster_nodes" {
  filter {
    name   = "vpc-id"
    values = [data.aws_eks_cluster.cluster.vpc_config[0].vpc_id]
  }
  filter {
    name   = "tag:Name"
    values = ["${local.cluster_name}-node"]
  }
}

locals {
  node_security_group_ids = distinct(concat(
    [data.aws_eks_cluster.cluster.vpc_config[0].cluster_security_group_id],
    data.aws_security_groups.cluster_nodes.ids,
  ))
}
