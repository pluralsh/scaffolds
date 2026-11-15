variable "cluster_handle" {
  type    = string
  default = "devbox-aws-karpenter"
}

variable "region" {
  type    = string
  default = "us-east-2"
}

variable "pool" {
  type    = string
  default = "devbox"
}

variable "provisioner" {
  type        = string
  default     = "karpenter"
  description = "karpenter or managed-node-group"

  validation {
    condition     = contains(["karpenter", "managed-node-group"], var.provisioner)
    error_message = "provisioner must be karpenter or managed-node-group."
  }
}

variable "sandbox_class" {
  type    = string
  default = "microvm"

  validation {
    condition     = contains(["gvisor", "microvm"], var.sandbox_class)
    error_message = "sandbox_class must be gvisor or microvm."
  }
}

variable "instance_types" {
  type    = string
  default = "c8id.4xlarge, m8id.4xlarge"
}

variable "disk_mode" {
  type        = string
  default     = "local"
  description = "local uses the instance store NVMe disks, network attaches an extra gp3 volume"
}

variable "boot_disk_size" {
  type    = number
  default = 100
}

variable "data_disk_size" {
  type    = number
  default = 200
}

variable "min_nodes" {
  type    = number
  default = 1
}

variable "max_nodes" {
  type    = number
  default = 10
}
