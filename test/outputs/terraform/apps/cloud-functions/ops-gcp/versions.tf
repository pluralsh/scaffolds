terraform {
  required_version = ">= 1.5"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 7.15"
    }
    # Deploying source without build (source_code on Cloud Run containers) is in beta.
    google-beta = {
      source  = "hashicorp/google-beta"
      version = ">= 7.15"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.40"
    }
  }
}

provider "google" {
  project = local.project_id
  region  = var.region
}

provider "google-beta" {
  project = local.project_id
  region  = var.region
}

provider "plural" {
}
