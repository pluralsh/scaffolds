resource "random_password" "password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "google_sql_database_instance" "mysql" {
  name             = var.name
  database_version = var.db_version
  project          = local.cluster_context.project_id
  region           = local.cluster_context.region

  settings {
    tier              = var.db_tier
    availability_type = var.db_availability_type

    backup_configuration {
      enabled            = true
      binary_log_enabled = true
      start_time         = "00:00"

      backup_retention_settings {
        retained_backups = var.backup_retention_days
      }
    }

    ip_configuration {
      ipv4_enabled       = false
      private_network    = data.google_compute_network.network.id
      allocated_ip_range = google_compute_global_address.private_ip_alloc.name
    }

    disk_size = var.db_storage_size_gb
    disk_type = var.db_storage_disk_type

    database_flags {
      name  = "character_set_server"
      value = "utf8mb4"
    }

    database_flags {
      name  = "collation_server"
      value = "utf8mb4_unicode_ci"
    }
  }

  deletion_protection = false

  depends_on = [
    data.plural_service_context.cluster,
    data.plural_service_context.network,
    google_service_networking_connection.mysql,
  ]
}

resource "google_sql_database" "database" {
  name      = var.name
  project   = local.cluster_context.project_id
  instance  = google_sql_database_instance.mysql.name
  charset   = "utf8mb4"
  collation = "utf8mb4_unicode_ci"
}

resource "google_sql_user" "user" {
  name     = var.db_username
  project  = local.cluster_context.project_id
  instance = google_sql_database_instance.mysql.name
  password = random_password.password.result
}