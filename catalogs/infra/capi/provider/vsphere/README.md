# CAPV provider setup

Installs the Cluster API Provider for vSphere (CAPV) on a management cluster via the Cluster API Operator.

Also writes **shared vSphere environment settings** to `helm/capi/vsphere-env/{cluster}/` used by workload cluster creation.

## Prerequisites

- **`capi-core-setup`** completed on the target management cluster
- vCenter or ESXi API reachable from the management cluster
- TLS thumbprint for the vSphere endpoint

## What this catalog creates

| Path | Purpose |
|------|---------|
| `helm/capi/vsphere-env/{cluster}/env.yaml` | Shared vSphere settings (server, datacenter, network, …) |
| `helm/capi/vsphere-env/{cluster}/secrets.yaml` | Shared vCenter password |
| `helm/capi/providers/vsphere/{cluster}/infrastructure-provider.yaml` | CAPV InfrastructureProvider CR |
| `helm/capi/providers/vsphere/{cluster}/credentials-secret.yaml` | vSphere credentials Secret |
| `bootstrap/capi/providers/vsphere/{cluster}/servicedeployment.yaml` | Plural service for CAPV |

## Next steps

After reconcile, confirm CAPV is running:

```bash
kubectl get infrastructureprovider -A
kubectl get pods -n capv-system
```

Then run **`capi-vsphere-cluster-creator`** to provision a workload cluster (no need to re-enter server/datacenter/password).
