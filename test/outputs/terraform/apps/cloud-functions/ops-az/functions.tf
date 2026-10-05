# Each app gets its own storage account for its package and the Functions host's state,
# including its keys. The app uses the account key, so a shared account would let any app read
# or replace the others' packages and keys. Identity-based access isn't used: the azurerm
# provider doesn't configure it correctly for Flex Consumption apps yet.
resource "azurerm_storage_account" "function" {
  for_each = local.functions

  name                            = local.storage_account_names[each.key]
  resource_group_name             = data.azurerm_resource_group.functions.name
  location                        = var.region
  account_tier                    = "Standard"
  account_replication_type        = "LRS"
  min_tls_version                 = "TLS1_2"
  allow_nested_items_to_be_public = false
  tags                            = var.tags
}

# The Functions host sends handler logs and invocations to Application Insights, which stores
# them in this workspace.
resource "azurerm_log_analytics_workspace" "functions" {
  name                = "${var.name}-functions-${substr(local.hash, 0, 6)}"
  resource_group_name = data.azurerm_resource_group.functions.name
  location            = var.region
  sku                 = "PerGB2018"
  retention_in_days   = var.log_retention_days
  tags                = var.tags
}

resource "azurerm_application_insights" "functions" {
  name                = "${var.name}-functions-${substr(local.hash, 0, 6)}"
  resource_group_name = data.azurerm_resource_group.functions.name
  location            = var.region
  workspace_id        = azurerm_log_analytics_workspace.functions.id
  application_type    = "other"
  tags                = var.tags
}

resource "azurerm_storage_container" "function" {
  for_each = local.functions

  name                  = local.app_names[each.key]
  storage_account_id    = azurerm_storage_account.function[each.key].id
  container_access_type = "private"
}

# The app's package. Flex Consumption apps run the package in their deployment container, which
# is how Azure's own deployments (One Deploy) publish them. azurerm's zip_deploy_file can't deploy
# to Flex Consumption: it waits for a Kudu endpoint those apps don't have.
resource "azurerm_storage_blob" "package" {
  for_each = local.functions

  name                 = "released-package.zip"
  storage_container_id = azurerm_storage_container.function[each.key].id
  type                 = "Block"
  source               = local.artifacts[each.key]
  content_md5          = filemd5(local.artifacts[each.key])
  content_type         = "application/zip"

  lifecycle {
    precondition {
      condition     = fileexists(local.artifacts[each.key])
      error_message = "${local.artifacts[each.key]} not found. The stack's fetch-functions init container downloads it; check that it ran and that ${var.artifact_version} contains ${each.value.binary}.zip."
    }
  }
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

# One app per function, so each runs as its own managed identity with only the actions it
# needs. A new package (see azurerm_storage_blob.package) restarts it.
resource "azurerm_function_app_flex_consumption" "function" {
  for_each = local.functions

  name                = local.app_names[each.key]
  resource_group_name = data.azurerm_resource_group.functions.name
  location            = var.region
  service_plan_id     = azurerm_service_plan.function[each.key].id

  storage_container_type      = "blobContainer"
  storage_container_endpoint  = "${azurerm_storage_account.function[each.key].primary_blob_endpoint}${azurerm_storage_container.function[each.key].name}"
  storage_authentication_type = "StorageAccountConnectionString"
  storage_access_key          = azurerm_storage_account.function[each.key].primary_access_key

  runtime_name           = "custom"
  runtime_version        = "1.0"
  instance_memory_in_mb  = var.instance_memory_in_mb
  maximum_instance_count = var.maximum_instance_count
  https_only             = true

  identity {
    type = "SystemAssigned"
  }

  site_config {
    application_insights_connection_string = azurerm_application_insights.functions.connection_string
  }

  # A changed package changes this setting, and changing settings restarts the app, which then
  # loads the new package.
  app_settings = merge(each.value.environment, {
    PLURAL_PACKAGE_MD5 = azurerm_storage_blob.package[each.key].content_md5
  })

  tags = var.tags
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

# Join actions of functions that reference networks, granted on the network resource groups
# without the functions' other actions.
resource "azurerm_role_definition" "network" {
  for_each = { for key, fn in local.functions : key => fn if length(fn.network_scopes) > 0 }

  name              = "${local.app_names[each.key]}-network"
  scope             = local.subscription_id
  description       = "Network join permissions of the ${local.app_names[each.key]} operational function."
  assignable_scopes = each.value.network_scopes

  permissions {
    actions = each.value.network_actions
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

# Reads ARM only serves at subscription scope, such as results of long-running actions.
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
