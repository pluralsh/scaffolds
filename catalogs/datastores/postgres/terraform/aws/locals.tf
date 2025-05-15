locals {
  db_name = var.name == "" ? "${data.cluster_name}-plural-db" : var.name
  db_url = format("postgresql://console:%s@%s:5432/console", random_password.password.result, coalesce(try(module.db.db_instance_address, ""), "ignore"))
  cluster_ready = {
    cluster = module.eks
    addons = module.eks_blueprints_addons
  }
  vpc_name = data.vpc_name == "" ? "${data.cluster_name}-vpc" : data.vpc_name
  monitoring_role_name = var.monitoring_role == "" ? "${data.cluster_name}-PluralRDSMonitoringRole" : var.monitoring_role
}