variable "cluster_name" {
  type = string
  default = "{{ context.cluster }}"
}

variable "db_name" {
  default = "plrl-{{ context.cluster }}-n8n"
}

variable "postgres_vsn" {
  default = "14"
}

variable "db_storage" {
  default = 20
}

variable "deletion_protection" {
  type    = bool
  default = true
}

variable "backup_retention_period" {
  type = number
  default = 7
}

variable "db_instance_class" {
  default = "db.t4g.medium"
}

