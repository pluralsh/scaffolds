# CAPV provider setup

Installs the Cluster API Provider for vSphere (CAPV) on a management cluster via the Cluster API Operator.

## Prerequisites

- **`capi-core-setup`** completed on the target management cluster
- vCenter or ESXi API reachable from the management cluster
- TLS thumbprint for the vSphere endpoint

## What this catalog creates

| Path | Purpose |
|------|---------|
| `bootstrap/capi/providers/vsphere/{cluster}/servicedeployment.yaml` | Plural service for CAPV |
| `charts/capi/providers/vsphere/` | CAPV Helm chart (InfrastructureProvider CR) |
| `helm/capi/providers/vsphere/{cluster}/values.yaml.liquid` | Non-secret vCenter settings |
| `helm/capi/providers/vsphere/{cluster}/secrets.yaml.liquid` | vCenter password (keep out of git if possible) |

## Next steps

After reconcile, confirm CAPV is running:

```bash
kubectl get infrastructureprovider -A
kubectl get pods -n capv-system
```

Then run **`capi-vsphere-cluster-creator`** to provision a workload cluster.
