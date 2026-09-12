resource "random_password" "database" {
  length      = 32
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "random_password" "admin" {
  length      = 32
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "random_id" "encryption_key" {
  byte_length = 32
}

resource "random_string" "database_suffix" {
  length  = 6
  special = false
  upper   = false
}
