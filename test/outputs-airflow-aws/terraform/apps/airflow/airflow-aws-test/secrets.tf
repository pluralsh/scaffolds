resource "random_bytes" "fernet" {
  length = 32
}

resource "random_password" "flask" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
}
