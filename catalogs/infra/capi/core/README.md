# CAPI core setup

Installs the [Cluster API Operator](https://cluster-api-operator.sigs.k8s.io/) and core Cluster API providers on a management cluster:

- **CoreProvider** (cluster-api)
- **BootstrapProvider** (kubeadm)
- **ControlPlaneProvider** (kubeadm)

This is provider-agnostic and only needs to run **once per management cluster** before installing infrastructure providers (CAPV, CAPA, CAPZ, etc.) or creating workload clusters.

Use a **cluster-api core** release (e.g. `v1.13.3`). Infrastructure providers such as CAPV use their own version line (e.g. `v1.16.1` via `capi-vsphere-provider-setup`).

## Prerequisites

- A registered Plural management cluster with kubectl access
- Helm support on the target cluster (Plural ServiceDeployment)

## What this catalog creates

| Path | Purpose |
|------|---------|
| `bootstrap/capi/core/{cluster}/servicedeployment.yaml` | Plural service to reconcile the operator |
| `helm/capi/core/capi-operator.yaml.liquid` | Helm values for core/bootstrap/control-plane versions |

## Next steps

After this PR merges and reconciles:

1. Confirm providers are ready: `kubectl get coreprovider,bootstrapprovider,controlplaneprovider -A`
2. Run **`capi-vsphere-provider-setup`** to install CAPV
3. Run **`capi-vsphere-cluster-creator`** to provision a workload cluster

## Alternatives

If you already bootstrapped CAPI with `clusterctl init`, you can skip this catalog.
