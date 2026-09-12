variable "hostname" {
  type = string
  default = "n8n-azure.example.org"
}

variable "cluster_name" {
  type = string
  default = "n8n-azure-test"
}

variable "resource_group_name" {
  type = string
  default = "n8n-test-rg"
}

variable "db_name" {
  default = "plrl-n8n-azure-test-n8n"
}

variable "db_disk" {
  type = number
  default = 32768
}

variable "db_sku" {
  type = string
  default = "GP_Standard_D2s_v3"
}
