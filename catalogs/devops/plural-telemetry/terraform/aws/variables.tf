variable "cluster_handle" {
  type    = string
  default = "{{ context.cluster }}"
}

variable "bucket_name" {
  type    = string
  default = "{{ context.bucket }}"
}

variable "region" {
  type    = string
  default = "{{ context.region }}"
}

variable "enable_versioning" {
  type        = bool
  default     = {% if context.versioning == false or context.versioning == "false" %}false{% else %}true{% endif %}
  description = "Whether to keep previous object versions so accidental deletes and overwrites can be recovered."
}

variable "version_retention_days" {
  type        = number
  default     = {{ context.versionRetentionDays | default: 7 }}
  description = "Days a non-current object version is kept before it expires. Expiry also applies to versions left over after versioning is turned off."

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
