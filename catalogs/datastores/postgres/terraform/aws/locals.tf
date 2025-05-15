locals {
  ctx_network = jsondecode(data.plural_service_context.network.configuration)
  db_url = format("postgresql://console:%s@%s:5432/console", random_password.password.result, coalesce(try(module.pg.db_instance_address, ""), "ignore"))
  monitoring_role_name = var.monitoring_role == "" ? "${var.name}-PluralRDSMonitoringRole" : var.monitoring_role
}