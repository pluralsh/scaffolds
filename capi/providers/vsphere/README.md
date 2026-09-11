# vSphere Cluster API controller

A helm chart that deploys the Cluster API Provider vSphere (CAPV) controller.

Plural's [capi-helm-charts](https://github.com/pluralsh/capi-helm-charts) repo does not yet publish a `cluster-api-provider-vsphere` subchart (unlike AWS, Azure, and GCP). This scaffold installs CAPV via a Cluster API Operator `InfrastructureProvider` resource and a credentials `Secret`.

## Prerequisites

- Cluster API core, kubeadm bootstrap, and kubeadm control-plane providers on the management cluster
- [Cluster API Operator](https://cluster-api-operator.sigs.k8s.io/) installed

Alternatively, install CAPV with `clusterctl init --infrastructure vsphere` and skip this chart.

## Values

| Key | Description |
|-----|-------------|
| `credentials.server` | vCenter or ESXi FQDN/IP |
| `credentials.username` | API user (e.g. `administrator@vsphere.local` or `root`) |
| `credentials.password` | API password |
| `credentials.thumbprint` | TLS thumbprint for the vSphere endpoint |
| `cluster-api-provider-vsphere.version` | CAPV release tag |
