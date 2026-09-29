terraform {
  required_version = ">= 1.5"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 6.0"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.38"
    }
  }
}

provider "google" {
  project = local.project_id
  region  = var.region
}

provider "plural" {
}
