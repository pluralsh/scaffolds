variable "name" {
  type        = string
  description = "Name of the Cloud Run function and its workbench tool, <installation>-<function key>."
}

variable "account_id" {
  type        = string
  description = "ID of the function's service account, at most 30 characters."
}

variable "tool_name" {
  type        = string
  description = "Name of the function's workbench tool, at most 40 characters."
}

variable "role_id" {
  type        = string
  description = "ID of the function's project custom role."
}

variable "function" {
  type = object({
    # Functions Framework entry point in the shared Go source package.
    entry_point = string
    # Tool description shown to the workbench agent.
    description = string
    memory      = string
    timeout     = number
    # Calls of the tools of destructive functions, which change or delete resources, need
    # human approval.
    destructive = bool
    # Function-specific environment. GOOGLE_CLOUD_PROJECT is always set.
    environment = map(string)
    # Minimal IAM permissions, granted through a project custom role.
    permissions = list(string)
    # JSON schema of the tool input.
    schema = string
    # Google APIs the function calls, enabled by the parent.
    apis = list(string)
  })
  description = "What the function needs, as defined in its <function key>.tf file."

  validation {
    condition     = length(var.function.permissions) > 0
    error_message = "A function needs at least one permission."
  }
}

variable "project_id" {
  type        = string
  description = "Project the function runs and acts in."
}

variable "region" {
  type        = string
  description = "Region of the Cloud Run function."
}

variable "build" {
  type = object({
    service_account_email = string
    docker_repository     = string
    bucket                = string
    object                = string
    generation            = number
  })
  description = "Shared build setup: the Cloud Build service account, the Artifact Registry repository and the source object, pinned to its generation."
}

variable "max_instance_count" {
  type        = number
  description = "Maximum number of instances the function scales out to."
}

variable "invoker_service_account" {
  type        = string
  description = "Email granted roles/run.invoker on the function, or null."
  default     = null
}

variable "cloud_connection_id" {
  type        = string
  description = "ID of the Plural cloud connection the workbench tool invokes the function with."
}

variable "labels" {
  type        = map(string)
  description = "Labels applied to every GCP resource."
  default     = {}
}
