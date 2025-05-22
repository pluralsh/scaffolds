terraform {
  required_providers {
    plural = {
      source = "pluralsh/plural"
      version = ">= 0.2.1"
    }
    google = {
      source  = "hashicorp/google"
      version = ">= 4.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "3.6.0"
    }
  }
}

provider "plural" {}

provider "google" {}