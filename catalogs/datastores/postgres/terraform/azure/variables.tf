variable "network" {
  description = "Network that database should belong to"
}

variable "name" {
  description = "Name of the database"
}

variable "db_disk" {
  type = number
  default = 32768
}

variable "db_sku" {
  type = string
  default = "GP_Standard_D4s_v3"
}
