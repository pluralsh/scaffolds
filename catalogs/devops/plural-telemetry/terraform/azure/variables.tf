variable "cluster_handle" {
  type    = string
  default = "{{ context.cluster }}"
}

variable "container_name" {
  type    = string
  default = "{{ context.bucket }}"
}

variable "storage_account_name" {
  type    = string
  default = "{{ context.storageAccount }}"
}
