terraform {
  required_version = ">= 1.0"

  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = ">= 3.51, < 5.0"
    }
    plural = {
      source  = "pluralsh/plural"
      version = ">= 0.2.9"
    }
  }
}

provider "plural" {}

data "plural_service_context" "identity" {
  name = "plrl/azure/identity"
}

locals {
  identity_context = jsondecode(data.plural_service_context.identity.configuration)
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
  client_id            = local.identity_context.client_id
  subscription_id      = local.identity_context.subscription_id
  tenant_id            = local.identity_context.tenant_id
}
