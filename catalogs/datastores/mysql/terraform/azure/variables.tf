variable "network" {
  description = "Network that database should belong to."
}

variable "name" {
  description = "Name of the database."
}

variable "db_version" {
  type = string
  default = "5.7"
  description = "The version of the MySQL Flexible Server to use. Possible values are 5.7, and 8.0.21. Changing this forces a new MySQL Flexible Server to be created."
}

variable "db_size_gb" {
  type = number
  default = 64
  description = "The max storage allowed for the MySQL Flexible Server. Possible values are between 20 and 16384."
}

variable "db_sku" {
  type = string
  default = "B_Gen5_2"
  description = "The SKU Name for the MySQL Flexible Server."
}

variable "db_username" {
  type = string
  default = "plural"
}
