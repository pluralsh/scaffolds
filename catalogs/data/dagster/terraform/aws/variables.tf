variable "cluster_handle" {
  type = string
  default = "{{ context.cluster }}"
}

variable "dagster_bucket" {
  type = string
  default = "{{ context.dagsterBucket }}"
}

variable "hostname" {
  type = string
  default = "{{ context.hostname }}"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = true
  description = "If true, the bucket will be deleted even if it contains objects."
}

variable "db_name" {
  default = "dagster"
}

variable "postgres_vsn" {
  default = "14"
}

variable "db_storage" {
  default = 20
}

variable "deletion_protection" {
  type    = bool
  default = false
}

variable "backup_retention_period" {
  type = number
  default = 7
}

variable "db_instance_class" {
  default = "db.t4g.large"
}