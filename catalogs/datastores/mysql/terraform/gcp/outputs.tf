output "private_ip_address" {
  value = google_sql_database_instance.mysql.private_ip_address
  description = "The private IP address of the MySQL instance"
}

output "public_ip_address" {
  value = google_sql_database_instance.mysql.public_ip_address
  description = "The private IP address of the MySQL instance"
}

output "db_name" {
  value = google_sql_database.database.name
  description = "The name of the database"
}

output "db_user" {
  value = google_sql_user.user.name
  description = "The database user"
  sensitive = true
}

output "db_password" {
  value = random_password.password.result
  description = "The database password"
  sensitive = true
}

output "db_url" {
  value = format("mysql://%s:%s@%s:3306/%s", 
    google_sql_user.user.name, 
    random_password.password.result, 
    google_sql_database_instance.mysql.private_ip_address, 
    google_sql_database.database.name
  )
  description = "The database connection URL"
  sensitive = true
}