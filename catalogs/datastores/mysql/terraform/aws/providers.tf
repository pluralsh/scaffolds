terraform {
  required_version = ">= 1.0"



  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 4.57"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = ">= 2.10"
    }
    random = {
      source  = "hashicorp/random"
      version = "3.6.0"
    }
    helm = {
      source = "hashicorp/helm"
      version = "2.14.0"
    }
    local = {
      source = "hashicorp/local"
      version = "2.5.2"
    }
    plural = {
      source = "pluralsh/plural"
      version = ">= 0.2.16"
    }
  }
}

provider "aws" {
}

provider "plural" {
}