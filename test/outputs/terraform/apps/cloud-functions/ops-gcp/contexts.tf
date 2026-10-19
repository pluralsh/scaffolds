# The project is the one of the GKE cluster var.cluster, from the service context the GCP
# bootstrap creates for each cluster it sets up: plrl/clusters/mgmt for the management cluster,
# plrl/clusters/<handle> for a workload cluster. The GCP project is project_id in its JSON
# configuration; the data source's own project_id is the Plural project.
data "plural_service_context" "cluster" {
  name = "plrl/clusters/${var.cluster}"

  lifecycle {
    postcondition {
      condition     = can(jsondecode(self.configuration).project_id)
      error_message = "The plrl/clusters/${var.cluster} service context has no project_id in its configuration. The cluster has to be a GKE cluster set up by the GCP bootstrap."
    }
  }
}
