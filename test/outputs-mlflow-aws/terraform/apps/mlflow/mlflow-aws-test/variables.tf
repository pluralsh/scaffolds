variable "cluster_name" {
  type = string
  default = "mlflow-aws-test"
}

variable "mlflow_bucket" {
  type = string
  default = "mlflow-artifacts-example-test"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = false
  description = "If true, the bucket will be deleted even if it contains objects."
}

variable "db_name" {
  default = "plrl-mlflow-aws-test-mlflow"
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
