# Node auto-provisioning (AKS's managed Karpenter) launches each devbox node in the zone its worker's home disk is in,
# from the NodePool and AKSNodeClass in the pool's service. With enable_nap the stack turns it on for the cluster.

locals {
  nap        = var.provisioner == "nap" && var.enable_nap ? 1 : 0
  network    = try(data.azurerm_kubernetes_cluster.cluster.network_profile[0], {})
  autoscaled = anytrue([for p in data.azurerm_kubernetes_cluster.cluster.agent_pool_profile : try(p.auto_scaling_enabled, false)])
  # empty with an AKS-managed VNet
  subnet_id         = try(coalesce(data.azurerm_kubernetes_cluster.cluster.agent_pool_profile[0].vnet_subnet_id), "")
  cluster_principal = try(data.azurerm_kubernetes_cluster.cluster.identity[0].principal_id, "")
  nap_feature       = "/subscriptions/${data.azurerm_client_config.current.subscription_id}/providers/Microsoft.Features/providers/Microsoft.ContainerService/features/AllowClusterAutoscalerAndNAP"
  # NAP runs beside cluster-autoscaled pools only with this subscription feature. The devbox NodePool's taint and label
  # keep the two from scaling for the same pods.
  register_nap_feature = local.nap == 1 && local.autoscaled && try(data.azapi_resource.nap_feature[0].output.properties.state, "") != "Registered"
}

data "azurerm_client_config" "current" {}

# what the azurerm data source doesn't expose
data "azapi_resource" "cluster" {
  type                   = "Microsoft.ContainerService/managedClusters@2025-10-01"
  resource_id            = data.azurerm_kubernetes_cluster.cluster.id
  response_export_values = ["properties.networkProfile.networkDataplane"]
}

data "azapi_resource" "nap_feature" {
  count                  = local.nap
  type                   = "Microsoft.Features/providers/features@2021-07-01"
  resource_id            = local.nap_feature
  response_export_values = ["properties.state"]
}

# Registering a feature needs subscription rights the stack's identity usually lacks; if this fails, run
# `az feature register --namespace Microsoft.ContainerService --name AllowClusterAutoscalerAndNAP` as a subscription
# owner. Once it's registered the stack skips this.
resource "azapi_resource_action" "nap_feature" {
  count       = local.register_nap_feature ? 1 : 0
  type        = "Microsoft.Features/providers/features@2021-07-01"
  resource_id = local.nap_feature
  action      = "register"
  method      = "POST"
}

# picks up the feature in the resource provider, as `az provider register` does
resource "azapi_resource_action" "nap_provider" {
  count       = local.register_nap_feature ? 1 : 0
  type        = "Microsoft.Resources/providers@2021-04-01"
  resource_id = "/subscriptions/${data.azurerm_client_config.current.subscription_id}/providers/Microsoft.ContainerService"
  action      = "register"
  method      = "POST"

  depends_on = [azapi_resource_action.nap_feature]
}

# No default NodePools: they're untainted, so NAP would add nodes for the same pending pods as the cluster autoscaler.
# Turn enable_nap off on a cluster where NAP already runs, since this would delete its default NodePools.
resource "azapi_update_resource" "nap" {
  count       = local.nap
  type        = "Microsoft.ContainerService/managedClusters@2025-10-01"
  resource_id = data.azurerm_kubernetes_cluster.cluster.id

  body = {
    properties = {
      nodeProvisioningProfile = {
        mode             = "Auto"
        defaultNodePools = "None"
      }
    }
  }

  # the feature takes a few minutes to reach AKS, which also rejects updates while another operation runs
  retry = {
    error_message_regex = ["AllowClusterAutoscalerAndNAP", "(?i)autoscaler", "(?i)operation.*in progress"]
  }

  timeouts {
    create = "40m"
    update = "40m"
  }

  lifecycle {
    precondition {
      condition     = try(local.network.network_plugin, "") == "azure" && try(local.network.network_policy, "") != "calico"
      error_message = "Node auto-provisioning needs Azure CNI (overlay or node subnet) without Calico network policy; use provisioner node-pools on this cluster."
    }
    precondition {
      condition     = length(data.azurerm_kubernetes_cluster.cluster.identity) > 0
      error_message = "Node auto-provisioning needs a cluster with a managed identity, not a service principal; use provisioner node-pools on this cluster."
    }
  }

  depends_on = [azapi_resource_action.nap_provider, azurerm_role_assignment.nap_subnet]
}

# NAP joins nodes to a custom VNet's subnet as the cluster identity, which needs to read and join it. A user-assigned
# cluster identity has to be granted that beforehand.
resource "azurerm_role_assignment" "nap_subnet" {
  count                = local.nap == 1 && local.subnet_id != "" && local.cluster_principal != "" ? 1 : 0
  scope                = local.subnet_id
  role_definition_name = "Network Contributor"
  principal_id         = local.cluster_principal
}
