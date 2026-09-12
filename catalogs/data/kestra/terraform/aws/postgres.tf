module "database_security_group" {
  source  = "terraform-aws-modules/security-group/aws"
  version = "~> 5.0"

  name        = "${local.resource_name}-db"
  description = "PostgreSQL access for Kestra from the cluster VPC"
  vpc_id      = local.vpc_id

  ingress_with_cidr_blocks = [
    {
      from_port   = 5432
      to_port     = 5432
      protocol    = "tcp"
      description = "PostgreSQL from the cluster VPC"
      cidr_blocks = local.vpc_cidr
    },
  ]

  tags = local.tags
}

module "database" {
  source  = "terraform-aws-modules/rds/aws"
  version = "~> 6.3"

  identifier = local.resource_name

  engine               = "postgres"
  engine_version       = var.postgres_version
  family               = "postgres${var.postgres_version}"
  major_engine_version = var.postgres_version
  instance_class       = var.db_instance_class

  allocated_storage     = var.db_storage
  max_allocated_storage = var.db_max_storage
  storage_encrypted     = true

  db_name                     = "kestra"
  username                    = "kestra"
  password                    = random_password.database.result
  manage_master_user_password = false
  port                        = 5432

  maintenance_window              = "Mon:00:00-Mon:03:00"
  backup_window                   = "03:00-06:00"
  backup_retention_period         = var.backup_retention_period
  enabled_cloudwatch_logs_exports = ["postgresql"]
  create_cloudwatch_log_group     = true

  monitoring_interval                   = 30
  monitoring_role_name                  = "${substr(local.resource_name, 0, 40)}-rds-monitoring"
  create_monitoring_role                = true
  performance_insights_enabled          = true
  performance_insights_retention_period = 7

  apply_immediately = true
  multi_az          = var.multi_az

  create_db_subnet_group = true
  subnet_ids             = local.subnet_ids
  vpc_security_group_ids = [module.database_security_group.security_group_id]

  parameters = [
    {
      name  = "autovacuum"
      value = "1"
    },
    {
      name  = "client_encoding"
      value = "utf8"
    },
  ]

  deletion_protection = var.deletion_protection
  tags                = local.tags
}
