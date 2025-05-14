resource "random_password" "password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

module "pg" {
  source  = "terraform-google-modules/sql-db/google//modules/postgresql"
  version = "~> 25.2"

  name                 = var.name
  random_instance_name = false
  project_id           = data.plural_service_context.mgmt.project_id
  database_version     = "POSTGRES_14"
  region               = data.plural_service_context.mgmt.region

  // Master configurations
  tier                            = var.size
  availability_type               = "REGIONAL"
  maintenance_window_day          = 7
  maintenance_window_hour         = 12
  maintenance_window_update_track = "stable"

  deletion_protection = false

  database_flags = [{ name = "autovacuum", value = "on" }]

  insights_config = {
    query_plans_per_minute = 5
  }

  ip_configuration = {
    ipv4_enabled                  = true
    private_network               = data.google_compute_network.network.id
    psc_enabled                   = false
    require_ssl                   = false
    allocated_ip_range            = google_compute_global_address.private_ip_alloc[0].name
    ssl_mode                      = "ENCRYPTED_ONLY"
  }

  backup_configuration = {
    enabled                        = true
    start_time                     = "20:55"
    location                       = null
    point_in_time_recovery_enabled = true
    transaction_log_retention_days = 7
    retained_backups               = 365
    retention_unit                 = "COUNT"
  }

  db_name      = var.name
  db_charset   = "UTF8"
  db_collation = "en_US.UTF8"

  user_name     = "admin"
  user_password = random_password.password.result

  depends_on = [
    data.plural_service_context.mgmt,
    data.plural_service_context.network,
  ]
}
