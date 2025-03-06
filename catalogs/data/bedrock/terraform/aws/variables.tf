variable "namespace" {
  description = "The Kubernetes namespace where the service account exists"
  type        = string
}

variable "cluster_name" {
  type    = string
  default = "plural"
}

variable "aws_region" {
  description = "The AWS region where Bedrock is deployed"
  type        = string
  default     = "us-east-1"
}
