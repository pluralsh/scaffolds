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

variable "cloud_connection_id" {
  type        = string
  description = "ID of the Plural cloud connection the workbench uses to invoke the functions."
}

variable "invoker_service_account" {
  type        = string
  description = "Email of the cloud connection service account. When set, it is granted roles/run.invoker on every function."
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

variable "artifact_version" {
  type        = string
  description = "Functions release to deploy, e.g. v0.1.0 (the functions/<version> tag in pluralsh/scaffolds). Used as the image tag."
}

# TODO(PROD-5251): default this to the public Plural Artifact Registry repository once it
# exists (see the images job in .github/workflows/functions-release.yaml).
variable "image_repository" {
  type        = string
  description = "Artifact Registry repository holding the function images as <repository>/<binary>:<version>."
  default     = null
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
