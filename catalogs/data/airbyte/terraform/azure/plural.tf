data "plural_cluster" "cluster" {
  handle = var.cluster_name
}

data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster_name}"
}
