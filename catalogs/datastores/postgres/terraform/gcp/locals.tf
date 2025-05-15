locals {
  db_url = format("postgresql://admin:%s@%s:5432/${var.name}", random_password.password.result, try(module.pg[0].private_ip_address, ""))
  ctx_network = jsondecode(data.plural_service_context.network.configuration)
  ctx_mgmt = jsondecode(data.plural_service_context.mgmt.configuration)
}