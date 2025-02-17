variable "cluster_name" {
  type = string
  default = "{{ context.cluster }}"
}

variable "osbuilder_bucket" {
  type = string
  default = "{{ context.bucket }}"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = true
  description = "If true, the bucket will be deleted even if it contains objects."
}