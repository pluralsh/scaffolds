variable "network" {
  description = "Network that database should belong to"
}

variable "name" {
  description = "Name of the database"
}

variable "db_version" {
  type = string
  default = "8.0.21"
  description = "The version of the MySQL Flexible Server to use, changing this forces a new MySQL Flexible Server to be created"
}

variable "db_size_gb" {
  type = number
  default = 32
  description = "The max storage allowed for the MySQL Flexible Server"
}

variable "db_sku" {
  type = string
  default = "GP_Standard_D2ds_v4"
  description = "The SKU Name for the MySQL Flexible Server"
}

variable "db_username" {
  type = string
  default = "plural"
}
