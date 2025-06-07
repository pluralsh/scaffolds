resource "random_password" "fernet" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "random_password" "flask" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
}