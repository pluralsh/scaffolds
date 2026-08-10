variable "cluster_handle" {
  type        = string
  description = "Plural handle of the target Kubernetes cluster."
  default     = "{{ context.cluster }}"
}

variable "storage_bucket" {
  type        = string
  description = "Globally unique GCS bucket for Kestra internal storage."
  default     = "{{ context.storageBucket }}"
}

variable "database_version" {
  type        = string
  description = "Cloud SQL PostgreSQL version supported by Kestra."
  default     = "POSTGRES_14"
}

variable "database_tier" {
  type        = string
  description = "Cloud SQL machine tier."
  default     = "db-custom-2-7680"
}

variable "availability_type" {
  type        = string
  description = "Cloud SQL availability mode."
  default     = "REGIONAL"
}

variable "deletion_protection" {
  type        = bool
  description = "Protect the Cloud SQL instance from accidental deletion."
  default     = true
}

variable "force_destroy_bucket" {
  type        = bool
  description = "Allow Terraform to delete a non-empty storage bucket."
  default     = false
}
