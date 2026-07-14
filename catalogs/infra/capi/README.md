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
└── cluster/vsphere/    # workload cluster values + ServiceDeployment templates
```

**Shared env:** `capi-vsphere-provider-setup` writes `helm/capi/vsphere-env/{mgmtCluster}/`. `capi-vsphere-cluster-creator` references it so vCenter settings are not re-entered.

**Charts:** Helm charts live in `capi/clusters/vsphere` and `capi/providers/vsphere` at the repo root — not under `catalogs/`. The cluster creator pulls the chart from the **scaffolds** GitRepository at deploy time (never copied through PrAutomation, to avoid Liquid conflicts with Helm `templates/_helpers.tpl`).
