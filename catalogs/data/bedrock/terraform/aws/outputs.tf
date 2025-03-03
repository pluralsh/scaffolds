output "bedrock_role_arn" {
  description = "IAM Role ARN for Bedrock access"
  value       = aws_iam_role.bedrock_role.arn
}

output "bedrock_policy_arn" {
  description = "IAM Policy ARN for Bedrock access"
  value       = aws_iam_policy.bedrock_policy.arn
}

output "bedrock_role_name" {
  description = "IAM Role Name for Bedrock"
  value       = aws_iam_role.bedrock_role.name
}

output "bedrock_assume_role_policy" {
  description = "Assume role policy for Bedrock role"
  value       = aws_iam_role.bedrock_role.assume_role_policy
}
