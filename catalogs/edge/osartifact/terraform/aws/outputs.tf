output "ecr_push_role_arn" {
  value = aws_iam_role.ecr_push_role.arn
}

output "repository_url" {
  value = data.aws_ecr_repository.ecr_repository.repository_url
}