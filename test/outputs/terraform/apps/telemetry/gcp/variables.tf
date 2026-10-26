variable "cluster_handle" {
  type    = string
  default = "gcp"
}

variable "bucket_name" {
  type    = string
  default = "plrl-telemetry"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = false
  description = "Whether Terraform may delete a non-empty Telemetry bucket."
}
