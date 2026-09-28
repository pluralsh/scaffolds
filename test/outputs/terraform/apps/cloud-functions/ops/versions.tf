terraform {
  required_version = ">= 1.5"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.40"
    }
    http = {
      source  = "hashicorp/http"
      version = ">= 3.4"
    }
    local = {
      source  = "hashicorp/local"
      version = ">= 2.5"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.38"
    }
  }
}

provider "aws" {
  region = var.region
}

provider "plural" {
}
