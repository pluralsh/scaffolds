terraform {
  required_version = ">= 1.0"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 6.10"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.9"
    }
  }
}

provider "google" {}

provider "plural" {}
