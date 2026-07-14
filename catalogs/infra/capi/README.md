# CAPI catalog

Plural service catalog for Cluster API workload clusters on vSphere.

## Automations

| PrAutomation | When to run |
|--------------|-------------|
| **`capi-core-setup`** | Once per management cluster — operator + core providers |
| **`capi-vsphere-provider-setup`** | Once per vCenter — install CAPV |
| **`capi-vsphere-cluster-creator`** | Per workload cluster |

## Layout

```
catalogs/infra/capi/
├── vsphere/            # shared vsphere-env templates
├── core/               # CAPI operator + core/bootstrap/control-plane
├── provider/vsphere/   # CAPV infrastructure provider
└── cluster/vsphere/    # kubeadm workload cluster chart
```

**Shared env:** `capi-vsphere-provider-setup` writes `helm/capi/vsphere-env/{mgmtCluster}/`. `capi-vsphere-cluster-creator` references it so vCenter settings are not re-entered.

Helm charts under `cluster/vsphere/chart` are vendored copies of `scaffolds/capi/clusters/vsphere`. The cluster creator references the chart from the **scaffolds** GitRepository (not copied into the infra repo) to avoid Liquid templating conflicts with Helm `templates/_helpers.tpl`.
