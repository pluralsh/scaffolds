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
# deployed file name.
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
  zip_deploy_file        = local_file.artifact[each.key].filename

  identity {
    type = "SystemAssigned"
  }

  site_config {}

  tags = var.tags
}

resource "azurerm_role_definition" "function" {
  for_each = { for key, fn in local.functions : key => fn if length(fn.actions) > 0 }

  name              = local.app_names[each.key]
  scope             = data.azurerm_resource_group.functions.id
  description       = "Permissions of the ${local.app_names[each.key]} operational function."
  assignable_scopes = [data.azurerm_resource_group.functions.id]

  permissions {
    actions = each.value.actions
  }
}

resource "azurerm_role_assignment" "function" {
  for_each = azurerm_role_definition.function

  scope              = data.azurerm_resource_group.functions.id
  role_definition_id = each.value.role_definition_resource_id
  principal_id       = azurerm_function_app_flex_consumption.function[each.key].identity[0].principal_id
}
