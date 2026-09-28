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

variable "cloud_connection_id" {
  type        = string
  description = "ID of the Plural cloud connection the workbench uses to invoke the functions."
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

variable "artifact_version" {
  type        = string
  description = "Functions release to deploy, e.g. v0.1.0 (the functions/<version> tag in pluralsh/scaffolds)."
}

variable "artifact_s3_bucket" {
  type        = string
  description = "S3 bucket in var.region hosting the release zips under functions/<version>/<binary>.zip. When null, zips are downloaded from artifact_base_url during the run instead (temporary, see README)."
  default     = null
}

variable "artifact_base_url" {
  type        = string
  description = "Base URL of the GitHub releases zips are downloaded from when artifact_s3_bucket is null."
  default     = "https://github.com/pluralsh/scaffolds/releases/download"
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
