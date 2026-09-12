variable "cluster_name" {
  type = string
  default = "n8n-gcp-test"
}

variable "db_name" {
  default = "plrl-n8n-gcp-test-n8n"
}

variable "database_version" {
  default = "POSTGRES_14"
}

variable "tier" {
  default = "db-perf-optimized-N-2"
}

variable "availability_type" {
  default = "REGIONAL"
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
  default = "n8n"
}
