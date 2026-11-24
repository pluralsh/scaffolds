terraform {
  required_version = ">= 1.3"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.95, < 6.0"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.9"
    }
  }
}

provider "aws" {
  region = var.region
}

provider "plural" {}
