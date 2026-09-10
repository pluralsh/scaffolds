variable "cluster_handle" {
  type    = string
  default = "{{ context.cluster }}"
}

variable "bucket_name" {
  type    = string
  default = "{{ context.bucket }}"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = false
  description = "Whether Terraform may delete a non-empty Tempo bucket."
}
