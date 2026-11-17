variable "cluster_handle" {
  type    = string
  default = "devbox-azure"
}

variable "resource_group_name" {
  type        = string
  default     = "workload-prod"
  description = "the AKS cluster's resource group, only used if the cluster context doesn't carry one"
}

variable "pool" {
  type    = string
  default = "devbox"

  validation {
    condition     = can(regex("^[a-z][a-z0-9]{0,11}$", var.pool))
    error_message = "AKS Linux node pool names must be 1-12 lowercase alphanumeric characters starting with a letter."
  }
}

variable "sandbox_class" {
  type    = string
  default = "gvisor"

  validation {
    condition     = contains(["gvisor", "microvm"], var.sandbox_class)
    error_message = "sandbox_class must be gvisor or microvm."
  }
}

variable "vm_size" {
  type    = string
  default = "Standard_L16s_v3"
}

variable "zones" {
  type    = list(string)
  default = ["1", "2", "3"]
}

variable "boot_disk_size" {
  type    = number
  default = 100
}

variable "min_nodes" {
  type    = number
  default = 1
}

variable "max_nodes" {
  type    = number
  default = 10
}

variable "install_substrate" {
  type        = bool
  default     = false
  description = "create Agent Substrate's snapshot storage and the workload identities of ate-api-server and atelet"
}
