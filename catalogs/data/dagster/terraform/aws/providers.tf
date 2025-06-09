terraform {
  required_providers {
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

provider "aws" {
  region = "{{ context.region }}"
}

# TODO: fix
# provider "kubernetes" {
#   host                   = "https://${data.google_container_cluster.cluster.endpoint}"
#   token                  = data.google_client_config.provider.access_token
#   cluster_ca_certificate = base64decode(data.google_container_cluster.cluster.master_auth[0].cluster_ca_certificate)
# }