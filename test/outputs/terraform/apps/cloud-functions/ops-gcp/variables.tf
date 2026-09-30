variable "name" {
  type        = string
  description = "Name of this installation, used as a prefix for every service, service account and workbench tool."

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{0,39}$", var.name))
    error_message = "name must start with a letter and contain at most 40 lowercase letters, digits or dashes."
  }
}

variable "region" {
  type        = string
  description = "GCP region to deploy the Cloud Run services to."
}

variable "cloud_connection" {
  type        = string
  description = "Name of the Plural cloud connection (GCP) the workbench uses to invoke the functions."
}

variable "invoker_service_account" {
  type        = string
  description = "Email of the cloud connection service account. When set, it is granted roles/run.invoker on every function."
  default     = null
}

variable "functions" {
  type        = list(string)
  description = "Functions to deploy, by key of local.catalog."
  default     = ["volume-delete"]

  validation {
    condition     = length(var.functions) > 0
    error_message = "At least one function must be deployed."
  }
}

variable "register_destructive_tools" {
  type        = bool
  description = "Register functions that change or delete resources as workbench tools. Every call of those tools then requires human approval."
  default     = false
}

variable "allow_skip_snapshot" {
  type        = bool
  description = "Let callers skip the snapshot taken before a disk is deleted. When false, volume-delete always snapshots first and its tool schema has no snapshot input."
  default     = false
}

variable "artifact_version" {
  type        = string
  description = "Functions release to deploy, e.g. v0.1.0 (the functions/<version> tag in pluralsh/scaffolds)."
}

variable "artifact_dir" {
  type        = string
  description = "Directory holding the release's function packages as <artifact_dir>/<artifact_version>/<binary>.tar.gz. The stack's init container downloads them there."
  default     = "/artifacts"
}

variable "max_instance_count" {
  type        = number
  description = "Maximum number of instances each service scales out to."
  default     = 5
}

variable "labels" {
  type        = map(string)
  description = "Labels applied to every GCP resource."
  default     = {}
}
