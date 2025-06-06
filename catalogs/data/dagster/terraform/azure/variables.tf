variable "resource_group_name" {
  type = string
  default = "{{ context.resourceGroupName }}"
}

variable "storage_account_name" {
  type = string
  default = "{{ context.storageAccountName }}"
}

variable "cluster_name" {
  type = string
  default = "{{ context.cluster }}"
}

variable "dagster_bucket" {
  type = string
  default = "{{ context.bucket }}"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = true
  description = "If true, the bucket will be deleted even if it contains objects."
}

variable "db_name" {
  default = "plrl-{{ context.cluster }}-dagster"
}

variable "postgres_vsn" {
  default = "14"
}

variable "db_disk" {
  type = number
  default = 32768
}

variable "db_sku" {
  type = string
  default = "GP_Standard_D4s_v3"
}