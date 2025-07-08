variable "cluster_handle" {
  type = string
  default = "{{ context.cluster }}"
}

variable "hostname" {
  type = string
  default = "{{ context.hostname }}"
}

variable "force_destroy_bucket" {
  type        = bool
  default     = true
  description = "If true, the bucket will be deleted even if it contains objects."
}

variable "db_name" {
  default = "dagster"
}

variable "dagster_bucket" {
  type = string
  default = "{{ context.dagsterBucket }}"
}

variable "database_version" {
  default = "POSTGRES_14"
}

variable "tier" {
  default = "db-perf-optimized-N-2"
}

variable "availability_type" {
  default = "REGIONAL"
  description = "Availability type for Cloud SQL instance"
}

variable "maintenance_window_day" {
  default = 7
}

variable "maintenance_window_hour" {
  default = 12
}

variable "maintenance_window_update_track" {
  default = "stable"
}

variable "deletion_protection" {
  type    = bool
  default = true
}

variable "database_flags" {
  default = { name = "autovacuum", value = "on" }
}

variable "insights_config" {
  default = { query_plans_per_minute = 5 }
}

variable "user_name" {
  default = "dagster"
}