variable "cluster_handle" {
  type        = string
  description = "Plural handle of the target Kubernetes cluster."
  default     = "{{ context.cluster }}"
}

variable "storage_account_name" {
  type        = string
  description = "Existing Azure Storage account in the cluster resource group."
  default     = "{{ context.storageAccount }}"
}

variable "storage_bucket" {
  type        = string
  description = "Azure Blob container for Kestra internal storage."
  default     = "{{ context.storageBucket }}"
}

variable "postgres_version" {
  type        = string
  description = "Azure PostgreSQL Flexible Server major version supported by Kestra."
  default     = "14"
}

variable "db_storage_mb" {
  type        = number
  description = "Initial PostgreSQL storage in MiB."
  default     = 32768
}

variable "db_sku" {
  type        = string
  description = "Azure PostgreSQL Flexible Server SKU."
  default     = "GP_Standard_D2s_v3"
}

variable "backup_retention_days" {
  type        = number
  description = "Number of days to retain PostgreSQL backups."
  default     = 7
}
