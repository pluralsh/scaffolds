data "aws_ecr_repository" "ecr_repository" {
  name = var.repository_name
}