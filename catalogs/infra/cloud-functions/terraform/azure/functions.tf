resource "azurerm_storage_account" "functions" {
  name                            = local.storage_account_name
  resource_group_name             = data.azurerm_resource_group.functions.name
  location                        = var.region
  account_tier                    = "Standard"
  account_replication_type        = "LRS"
  min_tls_version                 = "TLS1_2"
  allow_nested_items_to_be_public = false
  tags                            = var.tags
}

# Each app gets its own deployment container so deployments don't overwrite each other.
resource "azurerm_storage_container" "function" {
  for_each = local.functions

  name                  = local.app_names[each.key]
  storage_account_id    = azurerm_storage_account.functions.id
  container_access_type = "private"
}

# Flex Consumption allows a single app per plan.
resource "azurerm_service_plan" "function" {
  for_each = local.functions

  name                = local.app_names[each.key]
  resource_group_name = data.azurerm_resource_group.functions.name
  location            = var.region
  os_type             = "Linux"
  sku_name            = "FC1"
  tags                = var.tags
}

# One app per function, so each function runs as its own managed identity with only the
# actions it needs. Code updates are driven by artifact_version, which is part of the
# deployed package path.
resource "azurerm_function_app_flex_consumption" "function" {
  for_each = local.functions

  name                = local.app_names[each.key]
  resource_group_name = data.azurerm_resource_group.functions.name
  location            = var.region
  service_plan_id     = azurerm_service_plan.function[each.key].id

  storage_container_type      = "blobContainer"
  storage_container_endpoint  = "${azurerm_storage_account.functions.primary_blob_endpoint}${azurerm_storage_container.function[each.key].name}"
  storage_authentication_type = "StorageAccountConnectionString"
  storage_access_key          = azurerm_storage_account.functions.primary_access_key

  runtime_name           = "custom"
  runtime_version        = "1.0"
  instance_memory_in_mb  = var.instance_memory_in_mb
  maximum_instance_count = var.maximum_instance_count
  https_only             = true
  zip_deploy_file        = local.artifacts[each.key]

  identity {
    type = "SystemAssigned"
  }

  site_config {}

  app_settings = each.value.environment

  tags = var.tags

  lifecycle {
    precondition {
      condition     = fileexists(local.artifacts[each.key])
      error_message = "${local.artifacts[each.key]} not found. The stack's fetch-functions init container downloads it; check that it ran and that ${var.artifact_version} contains ${each.value.binary}.zip."
    }
  }
}

resource "azurerm_role_definition" "function" {
  for_each = { for key, fn in local.functions : key => fn if length(fn.actions) > 0 }

  name              = local.app_names[each.key]
  scope             = local.subscription_id
  description       = "Permissions of the ${local.app_names[each.key]} operational function."
  assignable_scopes = each.value.scopes

  permissions {
    actions = each.value.actions
  }

  lifecycle {
    precondition {
      condition     = length(each.value.scopes) > 0
      error_message = "${each.key} needs the resource groups it may act on: set scopes[\"${each.key}\"], e.g. to the AKS node resource group."
    }
  }
}

# The function's managed identity gets its role on each of its scopes only.
resource "azurerm_role_assignment" "function" {
  for_each = merge([
    for key, role in azurerm_role_definition.function : {
      for scope in local.functions[key].scopes : "${key}|${scope}" => { key = key, scope = scope, role = role.role_definition_resource_id }
    }
  ]...)

  scope              = each.value.scope
  role_definition_id = each.value.role
  principal_id       = azurerm_function_app_flex_consumption.function[each.value.key].identity[0].principal_id
  principal_type     = "ServicePrincipal"
  condition          = local.functions[each.value.key].condition
  condition_version  = local.functions[each.value.key].condition == null ? null : "2.0"
}

# Join actions on the resource groups of networks the function's resources reference, without
# its other actions there.
resource "azurerm_role_definition" "network" {
  for_each = { for key, fn in local.functions : key => fn if length(fn.network_scopes) > 0 }

  name              = "${local.app_names[each.key]}-network"
  scope             = local.subscription_id
  description       = "Network join permissions of the ${local.app_names[each.key]} operational function."
  assignable_scopes = each.value.network_scopes

  permissions {
    actions = each.value.network_actions
  }

  lifecycle {
    precondition {
      condition     = length(each.value.network_actions) > 0
      error_message = "${each.key} joins no networks: remove network_scopes[\"${each.key}\"]."
    }
  }
}

resource "azurerm_role_assignment" "network" {
  for_each = merge([
    for key, role in azurerm_role_definition.network : {
      for scope in local.functions[key].network_scopes : "${key}|${scope}" => { key = key, scope = scope, role = role.role_definition_resource_id }
    }
  ]...)

  scope              = each.value.scope
  role_definition_id = each.value.role
  principal_id       = azurerm_function_app_flex_consumption.function[each.value.key].identity[0].principal_id
  principal_type     = "ServicePrincipal"
}

# Reads ARM only serves at subscription scope, such as the results of long-running actions.
resource "azurerm_role_definition" "subscription" {
  for_each = { for key, fn in local.functions : key => fn if length(fn.subscription_actions) > 0 }

  name              = "${local.app_names[each.key]}-subscription"
  scope             = local.subscription_id
  description       = "Subscription-wide reads of the ${local.app_names[each.key]} operational function."
  assignable_scopes = [local.subscription_id]

  permissions {
    actions = each.value.subscription_actions
  }
}

resource "azurerm_role_assignment" "subscription" {
  for_each = azurerm_role_definition.subscription

  scope              = local.subscription_id
  role_definition_id = each.value.role_definition_resource_id
  principal_id       = azurerm_function_app_flex_consumption.function[each.key].identity[0].principal_id
  principal_type     = "ServicePrincipal"
}
