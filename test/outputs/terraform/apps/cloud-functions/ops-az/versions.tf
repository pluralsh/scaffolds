terraform {
  required_version = ">= 1.5"

  required_providers {
    # 4.77: storage_container_id on blobs, and maximum_instance_count below 40.
    azurerm = {
      source  = "hashicorp/azurerm"
      version = ">= 4.77"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.40"
    }
  }
}

provider "azurerm" {
  features {}

  use_cli              = false
  use_oidc             = true
  oidc_token_file_path = "/var/run/secrets/azure/tokens/azure-identity-token"
  subscription_id      = local.identity_context["subscription_id"]
  tenant_id            = local.identity_context["tenant_id"]
  client_id            = local.identity_context["client_id"]
}

provider "plural" {
}
