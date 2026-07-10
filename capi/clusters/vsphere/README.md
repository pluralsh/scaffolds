# vSphere workload cluster

A helm chart that deploys a kubeadm-based workload cluster on vSphere using Cluster API (CAPV).

Unlike AWS, Azure, and GCP scaffolds, vSphere uses a self-managed control plane (`KubeadmControlPlane` + `VSphereCluster`) rather than a cloud-managed API server.

## Prerequisites

- CAPV controller installed on the management cluster (`providers/vsphere` or `clusterctl init --infrastructure vsphere`)
- A VM template with kubeadm pre-installed (e.g. CAPV OVA imported to vSphere)
- A static VIP for the control plane endpoint when using kube-vip (default)

## Values

Chart defaults in `values.yaml` are configured for the Azure nested vSphere lab (`azure-lab`, proxy at `172.205.144.3`, `plural` datacenter, `VM Network`, kube-vip `192.168.100.50`). Override via `-f` for other environments.

Keep **passwords and SSH keys** in a separate values file (see `cluster-secrets.yaml.example` in the capv repo); do not commit secrets to the chart.

Key settings under `cluster.vsphere`:

| Key | Description |
|-----|-------------|
| `server` | vCenter or ESXi endpoint |
| `thumbprint` | TLS thumbprint |
| `datacenter` | vSphere datacenter |
| `datastore` | Datastore for VM disks |
| `network` | Port group name |
| `template` | VM template to clone |
| `controlPlaneEndpoint.host` | API server VIP or LB IP |
| `identity.username` / `identity.password` | vSphere credentials for this cluster |

Worker node pools are defined under `workers.groups`, following the same pattern as other Plural CAPI cluster charts.
