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
- `agent`: deploy the Alloy agents on every cluster (default on).
- `consoleSettings`: update `bootstrap/settings.yaml` so the Console reads logs and metrics from Telemetry (default on; see [Using Telemetry in the Console](#using-telemetry-in-the-console)).

### Bucket versioning and retention

The catalog creates the bucket private and encrypted, with a lifecycle rule that aborts incomplete multipart uploads. It does not enable versioning or any retention rule, and on Azure it does not change the storage account at all. Compaction constantly rewrites and deletes objects, so versioning raises storage cost unless old versions also expire.

If you need to recover deleted or overwritten data, turn on versioning and add a rule that expires non-current versions by editing the generated Terraform in your infra repo (`terraform/apps/telemetry/<cluster>/`), not the bucket directly. Terraform owns the bucket's whole lifecycle configuration, so a rule added by hand is removed on the next stack run. On an existing install that already has versioning enabled, check the Terraform plan: dropping the versioning resource suspends versioning but leaves already stored versions in the bucket, with no rule left to expire them.

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

The Console reads logs and metrics through its deployment settings. Unless `consoleSettings` is turned off, the PR this catalog generates updates `bootstrap/settings.yaml` so the Console uses Telemetry once the PR is merged and synced. The file must already exist in your infra repo, because the PR cannot be generated without it. It sets these fields (`<hostname>` and `<cluster>` are your install's hostname and cluster handle):

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

Things to know before you merge that PR:

- **It merges into the file.** It overrides the fields above, such as an Elasticsearch log driver or a Victoria Metrics host, and keeps every other key. Old blocks like `logging.elastic` stay, unused, and extra keys inside `logging.loki` or `prometheusConnection` (for example a `clusterLabel`) also stay. Delete stale ones yourself in the PR: the Console still resolves the secret of every connection block that is present, so a removed credential would break the whole settings sync, and a leftover `clusterLabel` makes cluster log views come back empty.
- **The file is rewritten.** Comments are removed, keys are sorted, and a second YAML document in the same file is dropped, so the diff can touch unrelated lines. This happens even when `consoleSettings` is off, though no values change then. Review the diff.
- **The Console switches over when the PR is merged and synced,** not when Telemetry is ready. Its logs and metrics views fail until the Telemetry stack is healthy and DNS and TLS work, and existing logs and metrics in the old backend are not migrated. To keep the Console on its current source until Telemetry is up, turn `consoleSettings` off and apply the snippet later.
- **Only one Telemetry instance should be wired to the Console.** Turn `consoleSettings` off (and `agent` off) for any additional instance, or the newest PR repoints the Console at it.

The generated credentials are also copied to the `plrl-deploy-operator` namespace as `telemetry-auth-<cluster>` (keys `username` and `password`), because the `global` `DeploymentSettings` there resolve secret references in their own namespace.

Then the Console's logs tabs and its node, pod and service metrics graphs read from Telemetry. The Console reads the password when its settings reconcile, so after you rotate it (by deleting the generated secret in `apps`) make any change to the `DeploymentSettings` to push the new one; until then its logs and metrics views return 401.

Point your DNS record for `hostname` at the ingress controller's address. The catalog does not create DNS records; the `externaldns` catalog can do that.

## Security notes

- All three products share one cloud identity and one `plrl` credential. A compromised pod or collector holding the password can read every product's data in the `default` telemetry namespace, and every cluster running the agent holds that password. Use separate read and write credentials (the stack chart's `authSecrets`) once collectors run outside Plural-managed clusters.
- On GCP the workload identity binding is `PROJECT.svc.id.goog[plrl-telemetry/<product>]`, which GKE treats as the same identity on every cluster in the project. Run one Telemetry per GCP project, or restrict who can create service accounts in `plrl-telemetry` on the other clusters.
- The cloud identity can delete and overwrite objects in the bucket, so a compromised pod can destroy stored telemetry. The catalog sets no versioning or retention that would let you restore it (see above).
- Inputs are validated before they are rendered into Terraform and YAML; keep those patterns if you edit the PrAutomation.

## Sizing

Metrics, Logs and Traces writers are not rolled out with less than 6Gi of memory. This catalog sets a 6Gi limit on every writer and a 4Gi limit on the metrics reader (the logs and traces readers use the operator default of 2Gi). The three writers and three readers therefore need roughly 26Gi of memory limits across the cluster's nodes, so make sure the target cluster has capacity. The Alloy agents add their own overhead on every cluster; tune the `collectors` section of `helm/telemetry/<cluster>/alloy.yaml.liquid` in your infra repo for large clusters.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
