variable "cluster_handle" {
  type    = string
  default = "{{ context.cluster }}"
}

variable "pool" {
  type    = string
  default = "{{ context.pool }}"
}

variable "sandbox_class" {
  type    = string
  default = "{{ context.sandboxClass }}"

  validation {
    condition     = contains(["gvisor", "microvm"], var.sandbox_class)
    error_message = "sandbox_class must be gvisor or microvm."
  }
}

variable "machine_type" {
  type    = string
  default = "{{ context.instanceTypes }}"
}

variable "disk_mode" {
  type        = string
  default     = "{{ context.diskMode }}"
  description = "local attaches raw local NVMe SSDs, network keeps /var/lib/ate on a larger boot disk"
}

variable "local_ssd_count" {
  type        = number
  default     = {{ context.localSsdCount | default: 2 }}
  description = "local NVMe SSDs per node (375GB each), only used when disk_mode is local"
}

variable "boot_disk_size" {
  type    = number
  default = {{ context.bootDiskSize }}
}

variable "boot_disk_type" {
  type        = string
  default     = "hyperdisk-balanced"
  description = "c3, c4 and n4 machines only support hyperdisk, n2 supports pd-balanced and pd-ssd"
}

variable "service_account" {
  type        = string
  default     = null
  description = "node service account email, defaults to the compute default service account"
}

variable "min_nodes" {
  type    = number
  default = {{ context.minNodes | default: 1 }}
}

variable "max_nodes" {
  type    = number
  default = {{ context.maxNodes | default: 10 }}
}

variable "install_substrate" {
  type        = bool
  default     = {% if context.substrate == true or context.substrate == "true" %}true{% else %}false{% endif %}
  description = "create Agent Substrate's snapshot storage and the workload identities of ate-api-server, atelet and devbox-zerofs"
}
