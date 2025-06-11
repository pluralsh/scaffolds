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

  name                 = local.db_name
  random_instance_name = false
  database_version     = var.database_version
  project_id           = local.project_id
  region = local.region

  // Master configurations
  edition                         = "ENTERPRISE"
  tier                            = var.tier
  availability_type               = var.availability_type
  maintenance_window_day          = var.maintenance_window_day
  maintenance_window_hour         = var.maintenance_window_hour
  maintenance_window_update_track = var.maintenance_window_update_track

  deletion_protection = var.deletion_protection

  database_flags = [var.database_flags]

  insights_config = var.insights_config

  ip_configuration = {
    ipv4_enabled       = true
    private_network    = data.google_compute_network.network.id
    psc_enabled        = false
    require_ssl        = false
    allocated_ip_range = google_compute_global_address.private_ip_alloc.name
    ssl_mode           = "ENCRYPTED_ONLY"
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

  db_name      = "dagster"
  db_charset   = "UTF8"
  db_collation = "en_US.UTF8"

  user_name     = var.user_name
  user_password = random_password.password.result

  depends_on = [
    data.plural_service_context.cluster,
    data.plural_service_context.network,
    google_service_networking_connection.postgres,
  ]
}
