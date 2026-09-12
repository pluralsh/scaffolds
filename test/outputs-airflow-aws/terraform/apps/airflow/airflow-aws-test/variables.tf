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