variable "cluster_handle" {
  type    = string
  default = "gcp-dev"
}

variable "bucket_name" {
  type    = string
  default = "plrl-telemetry"
}

variable "enable_versioning" {
  type        = bool
  default     = false
  description = "Whether to keep previous object versions so accidental deletes and overwrites can be recovered."
}

variable "version_retention_days" {
  type        = number
  default     = 7
  description = "Days a non-current object version is kept before it is deleted. Expiry also applies to versions left over after versioning is turned off."

  validation {
    condition     = var.version_retention_days >= 1 && floor(var.version_retention_days) == var.version_retention_days
    error_message = "version_retention_days must be a positive whole number."
  }
}

variable "force_destroy_bucket" {
  type        = bool
  default     = false
  description = "Whether Terraform may delete a non-empty Telemetry bucket."
}
