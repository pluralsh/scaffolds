variable "namespace" {
  description = "The Kubernetes namespace where the service account exists"
  type        = string
}

variable "service_account_name" {
  description = "The Kubernetes service account that should assume the IAM role"
  type        = string
}

variable "aws_region" {
  description = "The AWS region where Bedrock is deployed"
  type        = string
  default     = "us-east-1"
}
