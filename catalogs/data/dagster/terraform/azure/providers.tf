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
  }
}
provider "azurerm" {}

provider "plural" {}

provider "kubernetes" {
  host                   = "https://${data.azurerm_kubernetes_cluster.aks.kube_config.0.host}"
  cluster_ca_certificate = base64decode(data.azurerm_kubernetes_cluster.aks.kube_config.0.cluster_ca_certificate)
  client_certificate     = base64decode(data.azurerm_kubernetes_cluster.aks.kube_config.0.client_certificate)
  client_key             = base64decode(data.azurerm_kubernetes_cluster.aks.kube_config.0.client_key)
}