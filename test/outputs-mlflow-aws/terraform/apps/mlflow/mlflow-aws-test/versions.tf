
terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 4.57"
    }
    plural = {
      source = "pluralsh/plural"
      version = ">= 0.2.1"
    }
  }
}

provider "plural" {}

provider "aws" {
  region = "us-east-2"
}
