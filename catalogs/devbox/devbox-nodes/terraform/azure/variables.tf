variable "cluster_handle" {
  type    = string
  default = "{{ context.cluster }}"
}

variable "resource_group_name" {
  type        = string
  default     = "{{ context.resourceGroup }}"
  description = "the AKS cluster's resource group, only used if the cluster context doesn't carry one"
}

variable "pool" {
  type    = string
  default = "{{ context.pool }}"

  validation {
    condition     = can(regex("^[a-z]([-a-z0-9]{0,30}[a-z0-9])?$", var.pool))
    error_message = "pool must be 1-32 lowercase letters, digits and hyphens, starting with a letter."
  }
}

variable "provisioner" {
  type        = string
  default     = "{{ context.azureProvisioner | default: 'nap' }}"
  description = "nap (AKS node auto-provisioning) or node-pools"

  validation {
    condition     = contains(["nap", "node-pools"], var.provisioner)
    error_message = "provisioner must be nap or node-pools."
  }
}

variable "enable_nap" {
  type        = bool
  default     = {% if context.enableNap == false or context.enableNap == "false" %}false{% else %}true{% endif %}
  description = "turn on node auto-provisioning for the cluster, without AKS's default NodePools; off if NAP already runs there"
}

variable "sandbox_class" {
  type    = string
  default = "{{ context.sandboxClass }}"

  validation {
    condition     = contains(["gvisor", "microvm"], var.sandbox_class)
    error_message = "sandbox_class must be gvisor or microvm."
  }
}

variable "vm_size" {
  type    = string
  default = "{{ context.instanceTypes }}"
}

variable "zones" {
  type        = list(string)
  default     = ["1", "2", "3"]
  description = "with provisioner node-pools, the availability zones to run a node pool in, one each"

  validation {
    condition     = alltrue([for z in var.zones : can(regex("^[1-9]$", z))])
    error_message = "zones are single digits, e.g. 1, 2, 3."
  }
}

variable "boot_disk_size" {
  type    = number
  default = {{ context.bootDiskSize }}
}

variable "max_nodes" {
  type        = number
  default     = {{ context.maxNodes | default: 10 }}
  description = "with provisioner node-pools, the most nodes each zone's node pool may grow to"
}

variable "install_substrate" {
  type        = bool
  default     = {% if context.substrate == true or context.substrate == "true" %}true{% else %}false{% endif %}
  description = "create Agent Substrate's snapshot storage and the workload identities of ate-api-server and atelet"
}
