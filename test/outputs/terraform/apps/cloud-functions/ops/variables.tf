variable "name" {
  type        = string
  description = "Name of this installation, used as a prefix for every function, role and workbench tool."

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{0,39}$", var.name))
    error_message = "name must start with a letter and contain at most 40 lowercase letters, digits or dashes."
  }
}

variable "region" {
  type        = string
  description = "AWS region to deploy the functions to. Must match the region of the workbench cloud connection."
}

variable "cloud_connection" {
  type        = string
  description = "Name of the Plural cloud connection (AWS) the workbench uses to invoke the functions."
}

variable "functions" {
  type        = list(string)
  description = "Functions to deploy, by key of local.catalog."
  default     = ["volume-delete", "vm-delete", "node-pool-resize", "lb-delete", "db-restore", "ssh-access"]

  validation {
    condition     = length(var.functions) > 0
    error_message = "At least one function must be deployed."
  }
}

variable "allow_skip_snapshot" {
  type        = bool
  description = "Let callers skip the snapshot taken before a volume is deleted. When false, volume-delete always snapshots first and its tool schema has no snapshot input."
  default     = false
}

variable "node_pool_max_count" {
  type        = number
  description = "Largest desired size callers may set with node-pool-resize."
  default     = 100
  # Plural may pass null for optional stack variables left empty; use the default then.
  nullable = false

  validation {
    condition     = var.node_pool_max_count >= 0 && var.node_pool_max_count <= 1000
    error_message = "node_pool_max_count must be between 0 and 1000."
  }
}

variable "ssh_access_max_minutes" {
  type        = number
  description = "Longest access ssh-access may grant, in minutes."
  default     = 240
  # Plural may pass null for optional stack variables left empty; use the default then.
  nullable = false

  validation {
    condition     = var.ssh_access_max_minutes >= 1 && var.ssh_access_max_minutes <= 1440
    error_message = "ssh_access_max_minutes must be between 1 and 1440 (a day)."
  }
}

variable "artifact_version" {
  type        = string
  description = "Functions release to deploy, e.g. v0.1.0 (the functions/<version> tag in pluralsh/scaffolds)."
}

variable "artifact_dir" {
  type        = string
  description = "Directory holding the release's function packages as <artifact_dir>/<artifact_version>/<binary>.zip. The stack's init container downloads them there."
  default     = "/artifacts"
}

variable "log_retention_days" {
  type        = number
  description = "CloudWatch log retention for every function."
  default     = 30
}

variable "tags" {
  type        = map(string)
  description = "Tags applied to every AWS resource."
  default     = {}
}
