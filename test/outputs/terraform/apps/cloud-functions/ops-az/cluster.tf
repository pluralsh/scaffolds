# Scopes derived from cluster_resource_group: the functions act on the node pools of its AKS
# clusters (the resource group), their disks and load balancers (their node resource groups,
# MC_...), and join their node pools' subnets (the node resource group for an AKS-managed VNet).
data "azurerm_resources" "clusters" {
  count = var.cluster_resource_group == null ? 0 : 1

  resource_group_name = var.cluster_resource_group
  type                = "Microsoft.ContainerService/managedClusters"

  lifecycle {
    postcondition {
      condition     = length(self.resources) > 0
      error_message = "There is no AKS cluster in resource group ${var.cluster_resource_group}."
    }
  }
}

data "azurerm_kubernetes_cluster" "target" {
  for_each = toset(flatten([for r in data.azurerm_resources.clusters : r.resources[*].name]))

  name                = each.key
  resource_group_name = var.cluster_resource_group
}

locals {
  group_prefix = "/subscriptions/${local.identity_context["subscription_id"]}/resourceGroups/"
  clusters     = values(data.azurerm_kubernetes_cluster.target)

  cluster_group_id = var.cluster_resource_group == null ? null : "${local.group_prefix}${var.cluster_resource_group}"
  node_group_ids   = distinct([for c in local.clusters : "${local.group_prefix}${c.node_resource_group}"])
  # Resource groups of BYO node pool subnets; an AKS-managed VNet is in the node resource group.
  subnet_group_ids = distinct(flatten([
    for c in local.clusters : [
      for pool in c.agent_pool_profile : regex("(?i)^(/subscriptions/[^/]+/resourceGroups/[^/]+)/", pool.vnet_subnet_id)[0]
      if pool.vnet_subnet_id != null && pool.vnet_subnet_id != ""
    ]
  ]))
  vm_group_id       = var.vm_resource_group != null ? "${local.group_prefix}${var.vm_resource_group}" : local.cluster_group_id
  database_group_id = var.database_resource_group != null ? "${local.group_prefix}${var.database_resource_group}" : local.cluster_group_id

  cluster_scopes = var.cluster_resource_group == null ? tomap({}) : tomap({
    "volume-delete"      = local.node_group_ids
    "lb-frontend-delete" = local.node_group_ids
    "node-pool-resize"   = [local.cluster_group_id]
    "vm-delete"          = [local.vm_group_id]
    "ssh-access"         = [local.vm_group_id]
    "db-restore"         = [local.database_group_id]
  })
  scopes = merge(local.cluster_scopes, var.scopes)
  network_scopes = distinct(concat(
    var.network_scopes,
    [for g in var.network_resource_groups : "${local.group_prefix}${g}"],
    length(local.subnet_group_ids) > 0 ? local.subnet_group_ids : local.node_group_ids,
  ))
}
