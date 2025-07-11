variable "network" {
  description = "Network that postgres db should belong to"
}

variable "name" {
  description = "Name of the postgres db"
}

variable "db_name" {
  description = "Name of the database"
  default = "plural"
}

variable "postgres_vsn" {
  type = string
  default = "14"
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