variable "hostname" {
  type = string
  default = "{{ context.hostname }}"
}

variable "cluster_name" {
  type = string
  default = "{{ context.cluster }}"
}

variable "resource_group_name" {
  type = string
  default = "{{ context.resourceGroup }}"
}

variable "storage_account_name" {
  type = string
  default = "{{ context.storageAccount }}"
}

variable "db_name" {
  default = "plrl-{{ context.cluster }}-airbyte"
}

variable "db_disk" {
  type = number
  default = 32768
}

variable "db_sku" {
  type = string
  default = "GP_Standard_D4s_v3"
}
