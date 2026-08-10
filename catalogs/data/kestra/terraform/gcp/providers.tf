terraform {
  required_version = ">= 1.5"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 6.10, < 8.0"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.9"
    }
    random = {
      source  = "hashicorp/random"
      version = ">= 3.6, < 4.0"
    }
  }
}

provider "google" {}

provider "plural" {}
