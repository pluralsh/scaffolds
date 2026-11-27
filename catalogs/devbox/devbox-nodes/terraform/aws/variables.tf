variable "cluster_handle" {
  type    = string
  default = "{{ context.cluster }}"
}

variable "region" {
  type    = string
  default = "{{ context.region }}"
}

variable "pool" {
  type    = string
  default = "{{ context.pool }}"
}

variable "provisioner" {
  type        = string
  default     = "{{ context.awsProvisioner }}"
  description = "karpenter or managed-node-group"

  validation {
    condition     = contains(["karpenter", "managed-node-group"], var.provisioner)
    error_message = "provisioner must be karpenter or managed-node-group."
  }
}

variable "install_karpenter" {
  type        = bool
  default     = {% if context.installKarpenter == true or context.installKarpenter == "true" %}true{% else %}false{% endif %}
  description = "create the Karpenter controller's IAM, pod identity and interruption queue, once per cluster"
}

variable "sandbox_class" {
  type    = string
  default = "{{ context.sandboxClass }}"

  validation {
    condition     = contains(["gvisor", "microvm"], var.sandbox_class)
    error_message = "sandbox_class must be gvisor or microvm."
  }
}

variable "instance_types" {
  type    = string
  default = "{{ context.instanceTypes }}"
}

variable "disk_mode" {
  type        = string
  default     = "{{ context.diskMode }}"
  description = "local uses the instance store NVMe disks, network attaches an extra gp3 volume"
}

variable "boot_disk_size" {
  type    = number
  default = {{ context.bootDiskSize }}
}

variable "data_disk_size" {
  type    = number
  default = {{ context.dataDiskSize | default: 200 }}
}

variable "max_nodes" {
  type        = number
  default     = {{ context.maxNodes | default: 10 }}
  description = "the most nodes each zone's managed node group may grow to"
}

variable "install_substrate" {
  type        = bool
  default     = {% if context.substrate == true or context.substrate == "true" %}true{% else %}false{% endif %}
  description = "create Agent Substrate's snapshot storage and the workload identities of ate-api-server, atelet and devbox-zerofs"
}
