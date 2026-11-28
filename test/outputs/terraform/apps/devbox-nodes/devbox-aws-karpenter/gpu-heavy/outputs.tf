output "cluster_name" {
  value = local.cluster_name
}

output "cluster_endpoint" {
  value = data.aws_eks_cluster.cluster.endpoint
}

output "node_role_name" {
  value = local.node_role_name
}

output "subnet_ids" {
  value = join(",", local.subnet_ids)
}

output "node_security_group_ids" {
  value = join(",", local.node_security_group_ids)
}

output "interruption_queue" {
  value = try(module.karpenter[0].queue_name, "")
}

output "substrate_snapshot_location" {
  value = try("s3://${aws_s3_bucket.substrate[0].id}/substrate/", "")
}
