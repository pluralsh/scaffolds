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

variable "cloud_connection" {
  type        = string
  description = "Name of the Plural cloud connection (AZURE) the workbench uses to invoke the functions."
}

variable "resource_group_name" {
  type        = string
  description = "Resource group for the function apps. Defaults to the resource group of the mgmt cluster."
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

variable "scopes" {
  type        = map(list(string))
  description = "Resource groups each function may act on, by function key, as resource group IDs. Every deployed function needs at least one, e.g. the AKS node resource group (MC_...) for volume-delete and lb-frontend-delete."
  default     = {}

  validation {
    condition     = alltrue([for s in flatten(values(var.scopes)) : can(regex("^/subscriptions/[0-9a-fA-F-]{36}/resourceGroups/[^/]+$", s))])
    error_message = "scopes must be resource group IDs (/subscriptions/<id>/resourceGroups/<name>), not subscriptions or other resources."
  }
}

variable "node_pool_max_count" {
  type        = number
  description = "Largest node count node-pool-resize may set."
  default     = 100

  validation {
    condition     = var.node_pool_max_count >= 1 && var.node_pool_max_count <= 1000
    error_message = "node_pool_max_count must be between 1 and 1000."
  }
}

variable "ssh_access_max_minutes" {
  type        = number
  description = "Longest access ssh-access may grant, in minutes."
  default     = 240

  validation {
    condition     = var.ssh_access_max_minutes >= 1 && var.ssh_access_max_minutes <= 1440
    error_message = "ssh_access_max_minutes must be between 1 and 1440 (a day)."
  }
}

variable "ssh_bastion_id" {
  type        = string
  description = "Resource ID of the Azure Bastion host (Standard SKU or higher, with native client support) users connect through. ssh-access returns an az network bastion ssh command for it; without it, a direct az ssh vm command."
  default     = null

  validation {
    condition     = var.ssh_bastion_id == null || can(regex("^/subscriptions/[0-9a-fA-F-]{36}/resourceGroups/[^/]+/providers/Microsoft\\.Network/bastionHosts/[^/]+$", var.ssh_bastion_id))
    error_message = "ssh_bastion_id must be a Bastion host resource ID."
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
