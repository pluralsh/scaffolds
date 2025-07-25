terraform {
  required_version = ">= 1.0"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 6.10.0"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.9"
    }
    random = {
      source  = "hashicorp/random"
      version = ">= 3.0"
    }
  }
}
provider "google" {}

provider "plural" {}
