variable "name" {
  type        = string
  description = "Name of this installation, used as a prefix for every function app and workbench tool."

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{0,39}$", var.name))
    error_message = "name must start with a letter and contain at most 40 lowercase letters, digits or dashes."
  }
}

variable "region" {
  type        = string
  description = "Azure location to deploy the function apps to. It must support the Flex Consumption plan."
}

variable "cloud_connection_id" {
  type        = string
  description = "ID of the Plural cloud connection the workbench uses to invoke the functions."
}

variable "resource_group_name" {
  type        = string
  description = "Resource group for the function apps. Defaults to the resource group of the mgmt cluster."
  default     = null
}

variable "functions" {
  type        = list(string)
  description = "Functions to deploy, by key of local.catalog."
  default     = ["echo"]

  validation {
    condition     = length(var.functions) > 0
    error_message = "At least one function must be deployed."
  }
}

variable "register_destructive_tools" {
  type        = bool
  description = "Register functions that change or delete resources as workbench tools. Keep false until workbench tools can require approval (see workbench.tf)."
  default     = false
}

variable "allow_skip_snapshot" {
  type        = bool
  description = "Let callers skip the snapshot taken before a disk is deleted. When false, volume-delete always snapshots first and its tool schema has no snapshot input."
  default     = false
}

variable "volume_delete_scopes" {
  type        = list(string)
  description = "Resource groups volume-delete may snapshot and delete disks in, as resource group IDs, e.g. the AKS node resource group (MC_...). Required to deploy volume-delete."
  default     = []

  validation {
    condition     = alltrue([for s in var.volume_delete_scopes : can(regex("^/subscriptions/[0-9a-fA-F-]{36}/resourceGroups/[^/]+$", s))])
    error_message = "volume_delete_scopes must be resource group IDs (/subscriptions/<id>/resourceGroups/<name>), not subscriptions or other resources."
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

variable "instance_memory_in_mb" {
  type        = number
  description = "Flex Consumption instance memory for every function app."
  default     = 512
}

variable "maximum_instance_count" {
  type        = number
  description = "Maximum number of instances each function app scales out to."
  default     = 10
}

variable "tags" {
  type        = map(string)
  description = "Tags applied to every Azure resource."
  default     = {}
}
