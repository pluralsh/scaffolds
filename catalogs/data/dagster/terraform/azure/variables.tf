variable "storage_account_name" {
  type = string
  default = "{{ context.storageAccount }}"
}

variable "dagster_bucket" {
  type = string
}

variable "cluster_handle" {
  type = string
  default = "{{ context.cluster }}"
}

variable "hostname" {
  type = string
  default = "{{ context.hostname }}"
}

variable "db_name" {
  default = "dagster"
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