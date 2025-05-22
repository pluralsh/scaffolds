variable "network" {
  description = "Network that database should belong to"
}

variable "name" {
  description = "Name of the database"
}

variable "db_version" {
  type = string
  default = "MYSQL_8_0"
  description = "The MySQL version to use"
}

variable "db_tier" {
  type = string
  default = "db-n1-standard-2"
  description = "The tier for the MySQL instance"
}

variable "db_storage_size_gb" {
  type = number
  default = 20
  description = "The storage size in GB for the MySQL instance"
}

variable "db_storage_disk_type" {
  type = string
  default = "PD_SSD"
  description = "The type of storage disk for the MySQL instance (PD_SSD or PD_HDD)"
}

variable "db_availability_type" {
  type = string
  default = "REGIONAL"
  description = "The availability type for the MySQL instance (REGIONAL or ZONAL)"
}

variable "backup_retention_days" {
  type = number
  default = 7
  description = "The number of days to retain backups"
}

variable "db_username" {
  type = string
  default = "plural"
  description = "The username for the MySQL instance"
}