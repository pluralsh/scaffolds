# vSphere workload cluster

Provisions a kubeadm-based Kubernetes workload cluster on vSphere using Cluster API (CAPV).

## Prerequisites

- **`capi-core-setup`** on the management cluster
- **`capi-vsphere-provider-setup`** with CAPV running
- VM template with kubeadm pre-installed (matching the target Kubernetes version)
- Static VIP for kube-vip (control plane endpoint)

## What this catalog creates

| Path | Purpose |
|------|---------|
| `bootstrap/capi/clusters/{name}/servicedeployment.yaml` | Plural service to reconcile the cluster |
| `charts/capi/clusters/vsphere/` | Workload cluster Helm chart |
| `helm/capi/clusters/{name}/values.yaml.liquid` | Cluster topology and vSphere settings |
| `helm/capi/clusters/{name}/secrets.yaml.liquid` | Password and SSH keys |

Keep `secrets.yaml.liquid` out of version control or use a secrets manager where possible.

## Verify

```bash
kubectl get cluster,machine -A
kubectl get secret {name}-kubeconfig -o go-template='{{index .data "value"}}' | base64 -d
```
