# vSphere workload cluster

Provisions a kubeadm-based Kubernetes workload cluster on vSphere using Cluster API (CAPV).

## Prerequisites

- **`capi-core-setup`** on the management cluster
- **`capi-vsphere-provider-setup`** with shared env at `helm/capi/vsphere-env/{mgmtCluster}/`
- VM template with kubeadm pre-installed (matching the target Kubernetes version)
- Static VIP for kube-vip (control plane endpoint)

## What this catalog creates

| Path | Purpose |
|------|---------|
| `helm/capi/clusters/{name}/cluster.yaml` | Cluster-specific settings (name, template, endpoints, workers) |
| `helm/capi/clusters/{name}/secrets.yaml` | SSH keys |
| `bootstrap/capi/clusters/{name}/servicedeployment.yaml` | Plural service (Helm chart from `capi/clusters/vsphere` in scaffolds repo) |

vSphere connection settings (`server`, `datacenter`, `password`, etc.) are loaded from `helm/capi/vsphere-env/{mgmtCluster}/` via a ServiceDeployment `sources` entry (Plural multisource Helm cannot reference files outside the service git folder with `../` paths).

## Verify

```bash
kubectl get cluster,machine -A
kubectl get secret {name}-kubeconfig -o jsonpath='{.data.value}' | base64 -d
```
