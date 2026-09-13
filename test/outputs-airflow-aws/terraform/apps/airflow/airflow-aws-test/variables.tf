variable "cluster_name" {
  type = string
  default = "airflow-aws-test"
}

variable "airflow_bucket" {
  type = string
  default = "synthetic-airflow-logs"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = true
  description = "If true, the bucket will be deleted even if it contains objects."
}

variable "db_name" {
  default = "plrl-airflow-aws-test-airflow"
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
  default = "db.t4g.large"
}

variable "fernet_key" {
  type        = string
  default     = null
  sensitive   = true
  description = "Optional existing Fernet key or comma-separated new,old rotation list. When null, use the persistent generated key. Preserve the effective deployed key before migrating an existing database."

  validation {
    condition     = var.fernet_key == null ? true : can(regex("^[A-Za-z0-9_-]{43}=(,[A-Za-z0-9_-]{43}=)*$", var.fernet_key))
    error_message = "fernet_key must be null, a padded URL-safe base64-encoded 32-byte key, or a comma-separated list of such keys without spaces."
  }
}
