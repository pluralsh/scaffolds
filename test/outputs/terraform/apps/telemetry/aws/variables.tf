variable "cluster_handle" {
  type    = string
  default = "aws"
}

variable "bucket_name" {
  type    = string
  default = "plrl-telemetry"
}

variable "region" {
  type    = string
  default = "us-east-2"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = false
  description = "Whether Terraform may delete a non-empty Telemetry bucket."
}
