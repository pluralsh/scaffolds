# Shared vSphere environment

Connection and placement defaults for a management cluster's vSphere environment. Written once by **`capi-vsphere-provider-setup`** and reused by **`capi-vsphere-cluster-creator`**.

## Path

```
helm/capi/vsphere-env/{mgmtCluster}/
├── env.yaml      # server, thumbprint, datacenter, datastore, network, credentials, CAPV version
└── secrets.yaml  # password
```

## Updating

Re-run **`capi-vsphere-provider-setup`** to change vCenter settings or shared defaults. Workload clusters pick up changes on their next reconcile only where values are not overridden in per-cluster `cluster.yaml`.
