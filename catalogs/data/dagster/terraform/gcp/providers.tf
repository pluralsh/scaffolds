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

provider "kubernetes" {
  host                   = "https://${data.google_container_cluster.existing_cluster.endpoint}"
  token                  = data.google_client_config.provider.access_token
  cluster_ca_certificate = base64decode(data.google_container_cluster.existing_cluster.master_auth[0].cluster_ca_certificate)
}