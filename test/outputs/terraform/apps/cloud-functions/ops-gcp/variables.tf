variable "name" {
  type        = string
  description = "Name of this installation, used as a prefix for every function, service account and workbench tool."

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{0,39}$", var.name))
    error_message = "name must start with a letter and contain at most 40 lowercase letters, digits or dashes."
  }
}

variable "cluster" {
  type        = string
  description = "Handle of the GKE cluster whose project the functions are deployed to and act in, read from its plrl/clusters/<handle> service context."
  default     = "mgmt"
}

variable "region" {
  type        = string
  description = "GCP region to deploy the Cloud Run functions to."
}

variable "cloud_connection" {
  type        = string
  description = "Name of the Plural cloud connection (GCP) the workbench uses to invoke the functions."
}

variable "invoker_service_account" {
  type        = string
  description = "Email of the service account the cloud connection authenticates as. When set, it is granted roles/run.invoker on every function, so workbenches can call them. When null, grant that role yourself."
  default     = null
}

variable "functions" {
  type        = list(string)
  description = "Functions to deploy, by key of local.catalog in locals.tf."
  default     = ["volume-delete", "vm-delete", "node-pool-resize", "lb-frontend-delete", "db-restore", "ssh-access"]

  validation {
    condition     = length(var.functions) > 0
    error_message = "At least one function must be deployed."
  }
}

variable "artifact_version" {
  type        = string
  description = "Functions release to deploy, e.g. v0.1.0 (the functions/<version> tag in pluralsh/scaffolds)."
}

variable "artifact_dir" {
  type        = string
  description = "Directory holding the release's function source package as <artifact_dir>/<artifact_version>/functions-gcp.zip. The stack's init container downloads it there."
  default     = "/artifacts"
}

variable "release_retention_days" {
  type        = number
  description = "Days the source and images of earlier releases are kept. Source is deleted that long after a newer release replaced it, and images once they are that old, except for the 5 most recent versions of each function."
  default     = 30

  validation {
    condition     = var.release_retention_days >= 1
    error_message = "release_retention_days must be at least 1."
  }
}

variable "max_instance_count" {
  type        = number
  description = "Maximum number of instances each function scales out to."
  default     = 5
}

variable "labels" {
  type        = map(string)
  description = "Labels applied to every GCP resource."
  default     = {}
}
