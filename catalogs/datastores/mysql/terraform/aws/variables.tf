variable "network" {
  description = "Network that mysql db should belong to"
}

variable "name" {
  description = "Name of the mysql db"
}


variable "mysql_vsn" {
  type = string
  default = "8.0.35"
}

variable "db_instance_class" {
  default = "db.t4g.large"
}

variable "db_storage" {
  type = number
  default = 20
}

variable "backup_retention_period" {
  type = number
  default = 7
}

variable "deletion_protection" {
  type    = bool
  default = false
}

variable "monitoring_role" {
  type = string
  default = ""
}

variable "user_name" {
  default = "plural"
}