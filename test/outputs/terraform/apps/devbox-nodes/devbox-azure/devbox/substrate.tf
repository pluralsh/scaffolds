locals {
  substrate = var.install_substrate ? 1 : 0
  # Storage account names are 3-24 lowercase alphanumerics and globally unique.
  substrate_account_prefix = substr(replace(lower("ate${local.cluster_name}"), "/[^a-z0-9]/", ""), 0, 16)
  # The objectstore-azure-blob plugin runs next to atelet (node) and ate-api-server (control), and the ZeroFS
  # controller and node plugins (zerofs) keep devbox homes.
  # Each is its own Entra identity federated to that pod's ServiceAccount.
  substrate_identities = var.install_substrate ? {
    node    = "atelet"
    control = "ate-api-server"
    zerofs  = "devbox-zerofs"
  } : {}
}

resource "random_string" "substrate" {
  count   = local.substrate
  length  = 8
  upper   = false
  special = false
}

resource "azurerm_storage_account" "substrate" {
  count                           = local.substrate
  name                            = "${local.substrate_account_prefix}${random_string.substrate[0].result}"
  resource_group_name             = local.resource_group_name
  location                        = data.azurerm_kubernetes_cluster.cluster.location
  account_tier                    = "Standard"
  account_replication_type        = "LRS"
  min_tls_version                 = "TLS1_2"
  allow_nested_items_to_be_public = false
}

resource "azurerm_storage_container" "substrate" {
  count                 = local.substrate
  name                  = "snapshots"
  storage_account_id    = azurerm_storage_account.substrate[0].id
  container_access_type = "private"
}

resource "azurerm_user_assigned_identity" "substrate" {
  for_each            = local.substrate_identities
  name                = "${local.cluster_name}-${each.value}"
  resource_group_name = local.resource_group_name
  location            = data.azurerm_kubernetes_cluster.cluster.location
}

resource "azurerm_federated_identity_credential" "substrate" {
  for_each            = local.substrate_identities
  name                = "ate-system-${each.value}"
  resource_group_name = local.resource_group_name
  parent_id           = azurerm_user_assigned_identity.substrate[each.key].id
  audience            = ["api://AzureADTokenExchange"]
  issuer              = data.azurerm_kubernetes_cluster.cluster.oidc_issuer_url
  subject             = "system:serviceaccount:ate-system:${each.value}"
}

resource "azurerm_role_assignment" "substrate" {
  for_each             = local.substrate_identities
  scope                = azurerm_storage_account.substrate[0].id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.substrate[each.key].principal_id
}
