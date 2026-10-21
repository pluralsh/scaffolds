# Plural Telemetry

This catalog deploys [Plural Telemetry](https://github.com/pluralsh/telemetry): Prometheus-, Loki- and Tempo-compatible metrics, logs and traces stores built on [slatedb](https://slatedb.io/) with cloud object storage as the backend.

## What it deploys

- The `telemetry-operator` Helm chart (`oci://ghcr.io/pluralsh/charts`) into the `plrl-telemetry` namespace.
- The `telemetry-stack` Helm chart, which creates the `Metrics`, `Logs` and `Traces` resources (sharded mode, one writer and one reader each) and one `NamespaceAuthentication` per product.
- One object store for all three products: an S3 bucket, a GCS bucket, or an Azure Blob container. Each product keeps its data under its own path (`metrics/`, `logs/`, `traces/`).
- One ingress host with path routing and a single TLS certificate:
  - `https://<hostname>/metrics`
  - `https://<hostname>/logs`
  - `https://<hostname>/traces`
- Passwordless workload credentials for the `metrics`, `logs` and `traces` service accounts:
  - EKS Pod Identity on AWS.
  - Azure Workload Identity on AKS.
  - GKE Workload Identity on GCP.
- A generated basic auth secret (user `plrl`). A `GeneratedSecret` on the management cluster creates the password, the stack service receives it through `configurationRef`, and the chart creates `plrl-telemetry-auth` in `plrl-telemetry` on the target cluster. It is used for both reads and writes in the `default` telemetry namespace. The password is not stored in git or Terraform state.

Terraform provisions the bucket and cloud identity first. The Telemetry service imports those stack outputs into its Helm values, so bucket names and identity metadata are not duplicated or stored as static credentials.

## Configuration

- `cluster`: Kubernetes cluster to deploy Telemetry to.
- `cloud`: `aws`, `azure`, or `gcp`. It must match the target cluster's cloud.
- `bucket`: globally unique bucket name on AWS/GCP, or the Azure Blob container name.
- `hostname`: DNS name Telemetry is served under. Point it at your ingress controller.
- `ingressClass`: ingress class for the Telemetry ingresses (default `nginx`).
- `clusterIssuer`: cert-manager `ClusterIssuer` for the TLS certificate (default `letsencrypt-prod`).
- `region`: AWS region for the target EKS cluster and S3 bucket (AWS only).
- `storageAccount`: existing storage account in the target AKS cluster's resource group (Azure only).
- `versioning`: keep previous object versions so accidental deletes can be recovered (default on, AWS and GCP).
- `versionRetentionDays`: days a non-current version is kept before it expires (default `7`).
- `monitoring`: deploy `ServiceMonitor`s for the readers and writers (default off).

### Versioning

On AWS and GCP the bucket is versioned by default, and non-current versions expire after `versionRetentionDays`. Compaction rewrites and deletes objects constantly, so a long retention increases storage cost.

On Azure the catalog does not change the storage account, because blob versioning and lifecycle policies are account-wide settings and the account is shared. Enable blob versioning and a lifecycle rule on the account yourself if you want the same safety net.

### Monitoring

Telemetry serves its own metrics on `/metrics` (port `http`) but cannot scrape itself. The optional `monitoring` input deploys two `ServiceMonitor`s, which only work if the Prometheus operator CRDs (`monitoring.coreos.com/v1`) are installed on the target cluster and something scrapes them. Without the CRDs the service fails to sync, which is why it is off by default.

## Sending and reading data

All three products use the same credentials. Write and read through the matching path on the ingress host, in the `default` telemetry namespace, for example:

- Metrics read: `https://<hostname>/metrics/read/ns/default`
- Logs read: `https://<hostname>/logs/read/ns/default`

Read the password from the secret:

```bash
kubectl -n plrl-telemetry get secret plrl-telemetry-auth -o jsonpath='{.data.password}' | base64 -d
```

Clusters that push data use these endpoints and credentials, for example through an Alloy or OpenTelemetry collector. To use the same credentials elsewhere, copy that secret into the namespace that needs it (the original also lives in the `apps` namespace on the management cluster as `telemetry-auth-<cluster>`). This catalog does not deploy a collector; the `plural-telemetry-agent` catalog deploys Grafana Alloy on every cluster for that.

## Security notes

- All three products share one cloud identity and one `plrl` credential. A compromised pod or collector holding the password can read every product's data in the `default` telemetry namespace. Use separate read and write credentials (the stack chart's `authSecrets`) once collectors run outside the cluster.
- On GCP the workload identity binding is `PROJECT.svc.id.goog[plrl-telemetry/<product>]`, which GKE treats as the same identity on every cluster in the project. Run one Telemetry per GCP project, or restrict who can create service accounts in `plrl-telemetry` on the other clusters.
- The cloud identity can delete objects, including old versions, so a compromised pod can remove the version history. AWS is the exception, because its policy omits `s3:DeleteObjectVersion`.
- Inputs are validated before they are rendered into Terraform and YAML; keep those patterns if you edit the PrAutomation.

## Sizing

Metrics, Logs and Traces writers are not rolled out with less than 6Gi of memory. This catalog sets a 6Gi limit on every writer and a 4Gi limit on the metrics reader (the logs and traces readers use the operator default of 2Gi). The three writers and three readers therefore need roughly 26Gi of memory limits across the cluster's nodes, so make sure the target cluster has capacity.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
