variable "cluster_handle" {
  type        = string
  description = "Plural handle of the target Kubernetes cluster."
  default     = "{{ context.cluster }}"
}

variable "region" {
  type        = string
  description = "AWS region for the Kestra infrastructure."
  default     = "{{ context.region }}"
}

variable "storage_bucket" {
  type        = string
  description = "Globally unique S3 bucket for Kestra internal storage."
  default     = "{{ context.storageBucket }}"
}

variable "postgres_version" {
  type        = string
  description = "PostgreSQL major version supported by Kestra."
  default     = "14"
}

variable "db_instance_class" {
  type        = string
  description = "RDS instance class for the Kestra database."
  default     = "db.t4g.large"
}

variable "db_storage" {
  type        = number
  description = "Initial RDS storage in GiB."
  default     = 20
}

variable "db_max_storage" {
  type        = number
  description = "Maximum RDS autoscaled storage in GiB."
  default     = 100
}

variable "backup_retention_period" {
  type        = number
  description = "Number of days to retain RDS backups."
  default     = 7
}

variable "multi_az" {
  type        = bool
  description = "Whether to run RDS in a Multi-AZ configuration."
  default     = true
}

variable "deletion_protection" {
  type        = bool
  description = "Protect the RDS instance from accidental deletion."
  default     = true
}

variable "force_destroy_bucket" {
  type        = bool
  description = "Allow Terraform to delete a non-empty storage bucket."
  default     = false
}
