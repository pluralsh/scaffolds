# node-pool-resize: sets the node count of a manually scaled GKE node pool. See
# functions/go/gcp/docs/node-pool-resize.md.

variable "node_pool_max_count" {
  type        = number
  description = "Largest total node count (across the pool's zones) node-pool-resize may set."
  default     = 100
  # The stack passes the installation field as is, which is null when it was left empty; null
  # then means the default.
  nullable = false

  validation {
    condition     = var.node_pool_max_count >= 1 && var.node_pool_max_count <= 1000
    error_message = "node_pool_max_count must be between 1 and 1000."
  }
}

locals {
  node_pool_resize = {
    entry_point = "NodePoolResize"
    description = "Sets the node count of a manually scaled GKE node pool, per zone: the pool gets count nodes in each of its zones. GKE adds nodes, or drains and removes them, in the background. Pools scaled by the cluster autoscaler and Autopilot clusters are refused. Use action plan first to see the current and new node counts, then execute."
    memory      = "512Mi"
    timeout     = 30
    destructive = true
    apis        = ["container.googleapis.com", "compute.googleapis.com"]
    environment = {
      MAX_NODE_COUNT = tostring(var.node_pool_max_count)
    }
    # Every cluster in the project. Setting a pool's size needs container.clusters.update, which
    # also allows other cluster changes; only the function restricts it to resizing manually
    # scaled pools. The current node counts come from the pools' managed instance groups.
    permissions = [
      "container.clusters.get",
      "container.clusters.update",
      "compute.instanceGroupManagers.get",
    ]
    schema = jsonencode(jsondecode(file("${path.module}/schemas/node-pool-resize.json")))
  }
}
