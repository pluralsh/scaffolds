data "plural_service_context" "mgmt" {
  name = "plrl/clusters/mgmt"
}

data "plural_service_context" "network" {
  name = "plrl/vpc/${var.network}"
}