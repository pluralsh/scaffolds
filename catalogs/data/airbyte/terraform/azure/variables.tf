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

variable "db_dns_zone" {
  default = "plrl.postgres.database.azure.com"
}

variable "postgres_cidrs" {
  type = list(string)
  default = ["10.52.16.0/24"]
}

variable "network_name" {
  type = string
  default = "airbyte"
}

variable "network_cidrs" {
  type = list(string)
  default = ["10.52.0.0/16"]
}

variable "network_link_name" {
  default = "plrl.postgres.com"
}