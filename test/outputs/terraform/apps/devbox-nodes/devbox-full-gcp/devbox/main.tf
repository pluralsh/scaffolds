data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_handle}"
}

locals {
  cluster_context = jsondecode(data.plural_service_context.cluster.configuration)
}

resource "google_container_node_pool" "devbox" {
  name     = "${var.pool}-${var.sandbox_class}"
  project  = local.cluster_context.project_id
  location = local.cluster_context.region
  cluster  = local.cluster_context.cluster_name

  autoscaling {
    total_min_node_count = var.min_nodes
    total_max_node_count = var.max_nodes
    location_policy      = "ANY"
  }

  management {
    auto_repair  = true
    auto_upgrade = true
  }

  upgrade_settings {
    strategy        = "SURGE"
    max_surge       = 1
    max_unavailable = 0
  }

  node_config {
    machine_type    = var.machine_type
    image_type      = "UBUNTU_CONTAINERD"
    disk_size_gb    = var.boot_disk_size
    disk_type       = var.boot_disk_type
    service_account = var.service_account
    oauth_scopes    = ["https://www.googleapis.com/auth/cloud-platform"]

    labels = {
      "plural.sh/devbox-pool" = var.pool
    }

    taint {
      key    = "ate.dev/sandboxClass"
      value  = var.sandbox_class
      effect = "NO_SCHEDULE"
    }

    taint {
      key    = "plural.sh/devbox-disk"
      value  = "pending"
      effect = "NO_SCHEDULE"
    }

    dynamic "local_nvme_ssd_block_config" {
      for_each = var.disk_mode == "local" ? [1] : []
      content {
        local_ssd_count = var.local_ssd_count
      }
    }

    dynamic "advanced_machine_features" {
      for_each = var.sandbox_class == "microvm" ? [1] : []
      content {
        threads_per_core             = 0
        enable_nested_virtualization = true
      }
    }

    workload_metadata_config {
      mode = "GKE_METADATA"
    }

    shielded_instance_config {
      enable_secure_boot = true
    }
  }

  lifecycle {
    ignore_changes = [initial_node_count]
  }
}
