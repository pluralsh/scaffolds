locals {
  ctx_network = jsondecode(data.plural_service_context.network.configuration)
  db_url = format("mysql://%s:%s@%s:3306/%s", var.user_name, random_password.password.result, coalesce(try(module.mysql.db_instance_address, ""), "ignore"), var.name)
  monitoring_role_name = var.monitoring_role == "" ? "${var.name}-PluralRDSMonitoringRole" : var.monitoring_role
}