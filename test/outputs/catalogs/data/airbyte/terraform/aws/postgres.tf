resource "random_password" "password" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

data "aws_eks_cluster" "mgmt" {
  name = var.cluster_name
}

data "aws_vpc" "mgmt" {
  id = one(data.aws_eks_cluster.mgmt.vpc_config).vpc_id
}

module "db" {
  source = "terraform-aws-modules/rds/aws"
  version = "~> 6.3"

  identifier = var.db_name

  engine               = "postgres"
  engine_version       = var.postgres_vsn
  family               = "postgres14"
  major_engine_version = var.postgres_vsn 
  instance_class       = var.db_instance_class
  allocated_storage    = var.db_storage

  db_name  = "airbyte"
  username = "airbyte"
  password = random_password.password.result
  manage_master_user_password = false

  maintenance_window = "Mon:00:00-Mon:03:00"
  backup_window      = "03:00-06:00"
  backup_retention_period = var.backup_retention_period

  monitoring_interval    = "30"
  monitoring_role_name   = "${var.db_name}-PluralRDSMonitoringRole"
  create_monitoring_role = true
  apply_immediately      = true 

  multi_az = true

  create_db_subnet_group = true
  subnet_ids             = one(data.aws_eks_cluster.mgmt.vpc_config).subnet_ids
  vpc_security_group_ids = [module.security_group.security_group_id]

  create_cloudwatch_log_group = true
  enabled_cloudwatch_logs_exports = ["postgresql"]

  parameters = [
    {
      name  = "autovacuum"
      value = 1
    },
    {
      name  = "client_encoding"
      value = "utf8"
    }
  ]

  # Database Deletion Protection
  deletion_protection = var.deletion_protection
}

module "security_group" {
  source  = "terraform-aws-modules/security-group/aws"
  version = "~> 5.0"

  name        = "${var.db_name}-db-security-group"
  description = "security group for your plural console db"
  vpc_id      = data.aws_vpc.mgmt.id

  ingress_with_cidr_blocks = [
    {
      from_port   = 5432
      to_port     = 5432
      protocol    = "tcp"
      description = "PostgreSQL access from within VPC"
      cidr_blocks = data.aws_vpc.mgmt.cidr_block
    },
  ]
}
