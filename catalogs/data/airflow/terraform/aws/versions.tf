terraform {
  required_providers {
    random = {
      source  = "hashicorp/random"
      version = ">= 3.6.0, < 4.0"
    }
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

provider "aws" {}
