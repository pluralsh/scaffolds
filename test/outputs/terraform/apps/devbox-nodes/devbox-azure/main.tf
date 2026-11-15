data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context     = jsondecode(data.plural_service_context.cluster.configuration)
  cluster_name        = local.cluster_context.cluster_name
  resource_group_name = try(local.cluster_context.resource_group_name, var.resource_group_name)
}

data "azurerm_kubernetes_cluster" "cluster" {
  name                = local.cluster_name
  resource_group_name = local.resource_group_name
}

resource "azurerm_kubernetes_cluster_node_pool" "devbox" {
  name                  = var.pool
  kubernetes_cluster_id = data.azurerm_kubernetes_cluster.cluster.id
  vm_size               = var.vm_size
  zones                 = var.zones
  os_sku                = "Ubuntu"
  os_disk_size_gb       = var.boot_disk_size
  os_disk_type          = "Managed"
  vnet_subnet_id        = try(data.azurerm_kubernetes_cluster.cluster.agent_pool_profile[0].vnet_subnet_id, null)

  auto_scaling_enabled = true
  min_count            = var.min_nodes
  max_count            = var.max_nodes
  node_count           = var.min_nodes

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
