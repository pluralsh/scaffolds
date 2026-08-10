module "database" {
  source  = "terraform-google-modules/sql-db/google//modules/postgresql"
  version = "~> 25.2"

  name                 = local.resource_name
  random_instance_name = false
  database_version     = var.database_version
  project_id           = local.project_id
  region               = local.region

  edition           = "ENTERPRISE"
  tier              = var.database_tier
  availability_type = var.availability_type

  maintenance_window_day          = 7
  maintenance_window_hour         = 12
  maintenance_window_update_track = "stable"
  deletion_protection             = var.deletion_protection

  database_flags = [
    {
      name  = "autovacuum"
      value = "on"
    },
  ]

  ip_configuration = {
    ipv4_enabled    = false
    private_network = data.google_compute_network.network.id
    psc_enabled     = false
    ssl_mode        = "ENCRYPTED_ONLY"
  }

  backup_configuration = {
    enabled                        = true
    start_time                     = "03:00"
    location                       = null
    point_in_time_recovery_enabled = true
    transaction_log_retention_days = 7
    retained_backups               = 7
    retention_unit                 = "COUNT"
  }

  db_name      = "kestra"
  db_charset   = "UTF8"
  db_collation = "en_US.UTF8"

  user_name            = "kestra"
  user_password        = random_password.database.result
  user_deletion_policy = "ABANDON"
}
