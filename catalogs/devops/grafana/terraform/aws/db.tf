resource "random_password" "grafana" {
  length      = 20
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

data "plural_cluster" "cluster" {
  handle = var.cluster_name
}

data "aws_eks_cluster" "mgmt" {
  name = data.plural_cluster.cluster.name

  depends_on = [ data.plural_cluster.cluster ]
}

data "aws_vpc" "mgmt" {
  id = one(data.aws_eks_cluster.mgmt.vpc_config).vpc_id
}

module "db" {
  source = "terraform-aws-modules/rds/aws"
  version = "~> 6.3"

  identifier = "grafana"

  engine               = "postgres"
  engine_version       = "14"
  family               = "postgres14"
  major_engine_version = "14" 
  instance_class       = "db.t4g.small"
  allocated_storage    = 20

  db_name  = "grafana"
  username = "grafana"
  password = random_password.grafana.result
  manage_master_user_password = false

  maintenance_window = "Mon:00:00-Mon:03:00"
  backup_window      = "03:00-06:00"
  backup_retention_period = 7

  monitoring_interval    = "30"
  monitoring_role_name   = "grafana-PluralRDSMonitoringRole"
  create_monitoring_role = true

  multi_az = false

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
  deletion_protection = true
}

module "security_group" {
  source  = "terraform-aws-modules/security-group/aws"
  version = "~> 5.0"

  name        = "grafana-db-security-group"
  description = "security group for your plural grafana db"
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
