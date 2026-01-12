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

variable "db_name" {
  default = "plrl-{{ context.cluster }}-n8n"
}

variable "db_disk" {
  type = number
  default = 32768
}

variable "db_sku" {
  type = string
  default = "GP_Standard_D2s_v3"
}

