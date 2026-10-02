# lb-frontend-delete: removes what a deleted Kubernetes LoadBalancer Service left of its GKE load
# balancer. See functions/go/gcp/docs/lb-frontend-delete.md.

locals {
  lb_frontend_delete = {
    entry_point = "LBFrontendDelete"
    description = "Removes what a deleted Kubernetes LoadBalancer Service left of its GKE load balancer: the forwarding rule named after the Service UID, its target pool or backend service, health check, firewall rules and reserved address, as far as they were created for that Service. Before calling it, confirm in the cluster that no Service has a UID starting with the hex digits of the name. Refused while any backend is healthy. Use action plan first; each execute deletes what nothing uses any more, so execute again while the result reports remaining: true."
    memory      = "512Mi"
    timeout     = 30
    destructive = true
    apis        = ["compute.googleapis.com"]
    environment = {}
    # Every load balancer resource in the project; IAM can't restrict it to a Service's, so the
    # function checks names and descriptions itself. Firewall rules of a Shared VPC live in the
    # host project, where the function can't see or delete them.
    permissions = [
      "compute.forwardingRules.get",
      "compute.forwardingRules.list",
      "compute.forwardingRules.delete",
      "compute.targetPools.get",
      "compute.targetPools.getHealth",
      "compute.targetPools.delete",
      "compute.regionBackendServices.get",
      "compute.regionBackendServices.getHealth",
      "compute.regionBackendServices.delete",
      "compute.httpHealthChecks.get",
      "compute.httpHealthChecks.delete",
      "compute.healthChecks.get",
      "compute.healthChecks.delete",
      "compute.firewalls.get",
      "compute.firewalls.delete",
      "compute.addresses.get",
      "compute.addresses.delete",
    ]
    schema = jsonencode(jsondecode(file("${path.module}/schemas/lb-frontend-delete.json")))
  }
}
