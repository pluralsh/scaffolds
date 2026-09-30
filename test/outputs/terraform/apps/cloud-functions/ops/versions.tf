terraform {
  required_version = ">= 1.5"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.40"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.40"
    }
  }
}

provider "aws" {
  region = var.region
}

provider "plural" {
}
