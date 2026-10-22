# Plural Telemetry

This catalog deploys [Plural Telemetry](https://github.com/pluralsh/telemetry): Prometheus-, Loki- and Tempo-compatible metrics, logs and traces stores built on [slatedb](https://slatedb.io/) with cloud object storage as the backend.

## What it deploys

- The `telemetry-operator` service, which installs the `telemetry-operator` Helm chart (`oci://ghcr.io/pluralsh/charts`) into the `plrl-telemetry` namespace.
- The `telemetry` service, which installs the `telemetry-stack` Helm chart and creates the `Metrics`, `Logs` and `Traces` resources (sharded mode, one writer and one reader each) and one `NamespaceAuthentication` per product.
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
- Grafana Alloy agents on every cluster, including the management cluster (see [Agents](#agents)).

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
- `agent`: deploy the Alloy agents on every cluster (default on).

### Versioning

On AWS and GCP the bucket is versioned by default, and non-current versions expire after `versionRetentionDays`. Compaction rewrites and deletes objects constantly, so a long retention increases storage cost.

On Azure the catalog does not change the storage account, because blob versioning and lifecycle policies are account-wide settings and the account is shared. Enable blob versioning and a lifecycle rule on the account yourself if you want the same safety net.

## Agents

Unless `agent` is turned off, two global services deploy [Grafana Alloy](https://grafana.com/oss/alloy-opentelemetry-collector/) on every cluster managed by Plural, including the management cluster, into the `plrl-telemetry-agent` namespace:

- `telemetry-agent-operator` (Helm chart `alloy-operator` 0.8.0, with the Alloy CRDs).
- `telemetry-agent` (Helm chart `k8s-monitoring` 4.5.3). It collects:
  - cluster, node and kube-state metrics, plus ServiceMonitors and PodMonitors when the Prometheus operator CRDs exist;
  - pod logs and Kubernetes events;
  - OTLP metrics, logs and traces pushed by applications, on the Alloy OTLP receivers (gRPC and HTTP);
  - Telemetry's own `/metrics`. The writer and reader pods carry `k8s.grafana.com/scrape` annotations, so no Prometheus operator is needed.

Everything is sent to the ingress above, in the `default` telemetry namespace:

- Metrics: `https://<hostname>/metrics/write/ns/default/api/v1/write`
- Logs: `https://<hostname>/logs/write/ns/default/loki/api/v1/push`
- Traces: `https://<hostname>/traces/write/ns/default`

Metrics and logs carry the Plural cluster handle as the `cluster` label. On traces the `k8s-monitoring` chart sets it as a resource attribute, so filter traces on that attribute.

The agents reuse the generated credentials through `configurationRef` (secret `telemetry-auth-<cluster>` in the `apps` namespace on the management cluster), so no password is stored in git. If the secret does not exist yet, the global service is created once it does.

If you already run services named like these, or your own Alloy operator, remove them first or the Alloy CRDs and services will conflict. The agents are fleet-wide and have fixed names, so install them once. When you install a second Telemetry instance, turn `agent` off; otherwise its definitions overwrite the first, and every cluster's agents switch to the second instance's host and credentials.

## Sending and reading data

All three products use the same credentials. Write and read through the matching path on the ingress host, in the `default` telemetry namespace, for example:

- Metrics read: `https://<hostname>/metrics/read/ns/default`
- Logs read: `https://<hostname>/logs/read/ns/default`

Read the password from the secret:

```bash
kubectl -n plrl-telemetry get secret plrl-telemetry-auth -o jsonpath='{.data.password}' | base64 -d
```

The agents already push with these endpoints and credentials. Applications can send OTLP to the Alloy receivers on their own cluster, or push straight to the write endpoints above. To use the credentials elsewhere, copy that secret into the namespace that needs it (the original also lives in the `apps` namespace on the management cluster as `telemetry-auth-<cluster>`).

## Using Telemetry in the Console

The Console reads logs and metrics through its deployment settings, which this catalog does not change. The generated credentials are also copied to the `plrl-deploy-operator` namespace as `telemetry-auth-<cluster>` (keys `username` and `password`), where `<cluster>` is the handle you installed Telemetry on, because the `global` `DeploymentSettings` there resolve secret references in their own namespace. Set this in those settings (replace `<hostname>` and `<cluster>`):

```yaml
spec:
  logging:
    enabled: true
    driver: LOKI
    loki:
      host: https://<hostname>/logs/read/ns/default
      user: plrl
      passwordSecretRef:
        name: telemetry-auth-<cluster>
        key: password
  prometheusConnection:
    host: https://<hostname>/metrics/read/ns/default
    user: plrl
    passwordSecretRef:
      name: telemetry-auth-<cluster>
      key: password
```

Then the Console's logs tabs and its node, pod and service metrics graphs read from Telemetry. The Console reads the password when its settings reconcile, so after you rotate it (by deleting the generated secret in `apps`) make any change to the `DeploymentSettings` to push the new one; until then its logs and metrics views return 401. Only one Telemetry instance should be wired to the Console, like the agents.

Point your DNS record for `hostname` at the ingress controller's address. The catalog does not create DNS records; the `externaldns` catalog can do that.

## Security notes

- All three products share one cloud identity and one `plrl` credential. A compromised pod or collector holding the password can read every product's data in the `default` telemetry namespace, and every cluster running the agent holds that password. Use separate read and write credentials (the stack chart's `authSecrets`) once collectors run outside Plural-managed clusters.
- On GCP the workload identity binding is `PROJECT.svc.id.goog[plrl-telemetry/<product>]`, which GKE treats as the same identity on every cluster in the project. Run one Telemetry per GCP project, or restrict who can create service accounts in `plrl-telemetry` on the other clusters.
- The cloud identity can delete objects, including old versions, so a compromised pod can remove the version history. AWS is the exception, because its policy omits `s3:DeleteObjectVersion`.
- Inputs are validated before they are rendered into Terraform and YAML; keep those patterns if you edit the PrAutomation.

## Sizing

Metrics, Logs and Traces writers are not rolled out with less than 6Gi of memory. This catalog sets a 6Gi limit on every writer and a 4Gi limit on the metrics reader (the logs and traces readers use the operator default of 2Gi). The three writers and three readers therefore need roughly 26Gi of memory limits across the cluster's nodes, so make sure the target cluster has capacity. The Alloy agents add their own overhead on every cluster; tune the `collectors` section of `helm/telemetry/<cluster>/alloy.yaml.liquid` in your infra repo for large clusters.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
