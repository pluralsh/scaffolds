data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context     = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name        = local.cluster_context.cluster_name
  resource_group_name = try(local.cluster_context.resource_group_name, var.resource_group_name)
  # AKS Linux node pool names are 1-12 lowercase alphanumerics, here the pool's first 11 and the zone; the node label
  # keeps the full pool name.
  node_pool_prefix = substr(replace(var.pool, "-", ""), 0, 11)
}

data "azurerm_kubernetes_cluster" "cluster" {
  name                = local.cluster_name
  resource_group_name = local.resource_group_name
}

# With provisioner node-pools, one node pool per zone: a pool spanning zones is one scale set that picks zones itself,
# and the cluster autoscaler isn't zone-aware, so a worker pinned to its homes' zone could wait on a node in another.
resource "azurerm_kubernetes_cluster_node_pool" "devbox" {
  for_each              = var.provisioner == "node-pools" ? toset(var.zones) : toset([])
  name                  = "${local.node_pool_prefix}${each.key}"
  kubernetes_cluster_id = data.azurerm_kubernetes_cluster.cluster.id
  vm_size               = var.vm_size
  zones                 = [each.key]
  os_sku                = "Ubuntu"
  os_disk_size_gb       = var.boot_disk_size
  os_disk_type          = "Managed"
  vnet_subnet_id        = try(data.azurerm_kubernetes_cluster.cluster.agent_pool_profile[0].vnet_subnet_id, null)

  # Zones start empty and each may take the whole pool. The runner's minReplicas and maxReplicas bound the workers
  # across zones and decide which zone keeps the warm ones; a per-zone minimum would keep idle nodes in every zone.
  auto_scaling_enabled = true
  min_count            = 0
  max_count            = var.max_nodes
  node_count           = 0

  node_labels = {
    "plural.sh/devbox-pool" = var.pool
  }

  node_taints = [
    "ate.dev/sandboxClass=${var.sandbox_class}:NoSchedule",
    "plural.sh/devbox-disk=pending:NoSchedule",
  ]

  upgrade_settings {
    max_surge = "1"
  }

  lifecycle {
    ignore_changes = [node_count]
  }
}
