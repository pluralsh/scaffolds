data "plural_service_context" "identity" {
  name = "plrl/azure/identity"
}

data "plural_service_context" "network" {
  name = "plrl/vpc/${var.network}"
}

data "plural_service_context" "cluster" {
  name = "plrl/clusters/mgmt"
}
