locals {
  db_url = format("postgresql://admin:%s@%s:5432/${var.name}", random_password.password.result, try(module.pg[0].private_ip_address, ""))
}