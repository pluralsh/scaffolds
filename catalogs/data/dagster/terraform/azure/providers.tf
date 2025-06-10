terraform {
  required_version = ">= 1.0"

  required_providers {
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.9"
    }
    random = {
      source  = "hashicorp/random"
      version = ">= 3.0"
    }
    azurerm = {
      source  = "hashicorp/azurerm"
      version = ">= 3.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = ">= 2.10"
    }
  }
}

provider "azurerm" {
  features {
    resource_group {
      prevent_deletion_if_contains_resources = false
    }
  }

  use_cli              = false
  use_oidc             = true
  oidc_token_file_path = "/var/run/secrets/azure/tokens/azure-identity-token"
  client_id            = local.client_id
  subscription_id      = local.subscription_id
  tenant_id            = local.tenant_id
}

provider "kubernetes" {
  host = "https://${data.azurerm_kubernetes_cluster.aks.fqdn}"
  cluster_ca_certificate = base64decode(data.azurerm_kubernetes_cluster.aks.kube_config.0.cluster_ca_certificate)
  client_certificate = base64decode(data.azurerm_kubernetes_cluster.aks.kube_config.0.client_certificate)
  client_key = base64decode(data.azurerm_kubernetes_cluster.aks.kube_config.0.client_key)
}