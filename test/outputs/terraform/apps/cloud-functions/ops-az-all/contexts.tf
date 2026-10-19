data "plural_service_context" "identity" {
  name = "plrl/azure/identity"
}

data "plural_service_context" "cluster" {
  name = "plrl/clusters/mgmt"
}

data "azurerm_resource_group" "functions" {
  name = local.resource_group_name
}
