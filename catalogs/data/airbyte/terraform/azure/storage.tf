data "azurerm_storage_account" "airbyte" {
  name                = "{{ context.storageAccount }}"
  resource_group_name = "{{ context.resourceGroup }}"
}