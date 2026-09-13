# Plural Pushgateway

Deploys [Prometheus Pushgateway](https://github.com/prometheus/pushgateway)
for service-level batch jobs that finish before Prometheus can scrape them.
It is not a general replacement for scraping long-running services. Review
the upstream [guidance on when to use a Pushgateway](https://prometheus.io/docs/practices/pushing/).

## Deployment

- Upstream `prometheus-pushgateway` chart `3.8.0`, application `v1.11.3`.
- One replica in namespace `pushgateway`, with a `Recreate` update strategy.
- A 2Gi ReadWriteOnce volume mounted at `/data`; metrics are periodically
  saved to `/data/pushgateway.data` every five seconds.
- Internal ClusterIP service `pushgateway` on port 9091. No Ingress,
  HTTPRoute, ServiceMonitor, external endpoint, or Prometheus server is created.
- A dedicated ServiceAccount with token automount disabled and no RBAC grants.
- UID/GID 65534, a read-only root filesystem, dropped Linux capabilities,
  no privilege escalation, and the runtime's default seccomp profile.

## Inputs and prerequisites

| Input | Meaning |
| --- | --- |
| `cluster` | Target cluster with a writable ReadWriteOnce storage provisioner. |
| `webConfigSecret` | Existing Secret name in namespace `pushgateway`. Omitted or whitespace-only input uses `pushgateway-web-config`. |
| `storageClass` | Optional StorageClass name; omitted or blank uses the cluster default. |

The Secret must contain a `web-config.yaml` key. Prepare this file locally,
following the [exporter-toolkit web configuration format](https://github.com/prometheus/exporter-toolkit/blob/master/docs/web-configuration.md):

```yaml
basic_auth_users:
  batch: "<bcrypt hash generated for your own password>"
```

Replace the placeholder with a bcrypt hash for a password you choose. The
`basic_auth_users` map must contain at least one user. An existing Secret name
alone does not prove its contents enable authentication. Do not use an empty
configuration, and do not commit this file or a plaintext password to Git.

Create the namespace if it does not exist, then provision the Secret through
your normal secret-management process. With the target Kubernetes context
selected, an initial manual setup is:

```sh
kubectl create namespace pushgateway --dry-run=client -o yaml | kubectl apply -f -
kubectl -n pushgateway create secret generic pushgateway-web-config --from-file=web-config.yaml
```

For a custom Secret name, use the same name in that command and the catalog
input. If the Secret is missing, Kubernetes cannot mount it and the pod will
not start. The chart references this existing Secret; the generated Helm
values contain only its name. Blank input keeps that requirement in place.

The automation writes its ServiceDeployment under
`bootstrap/apps/pushgateway/<cluster>/` and Helm values under
`helm/pushgateway/<cluster>/`. It uses the existing `infra` GitRepository in
namespace `infra`, consistent with the other catalog applications.

## Verify before connecting jobs

For the existing-Secret mode, chart 3.8.0 uses TCP liveness/readiness probes
because it cannot read credentials while rendering the manifests. A ready pod
therefore proves a listening port, not a valid authentication configuration.
Verify authenticated HTTP access and rejection of unauthenticated access:

```sh
kubectl -n pushgateway port-forward service/pushgateway 9091:9091
```

In another terminal, these unauthenticated requests should return HTTP 401:

```sh
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:9091/metrics
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:9091/-/ready
```

These commands prompt for the password of the example user `batch`:

```sh
curl --fail --user batch http://localhost:9091/-/ready
printf 'catalog_probe_success 1\n' | curl --fail --user batch --data-binary @- http://localhost:9091/metrics/job/catalog_probe
curl --fail --user batch http://localhost:9091/metrics
curl --fail --user batch -X DELETE http://localhost:9091/metrics/job/catalog_probe
```

Use a job name reserved for this check. The DELETE request removes that
grouping key only. All authenticated users can push, read, and delete metrics;
basic authentication does not provide per-job authorization. The global
administration API remains disabled.

## Prometheus integration

Configure your existing Prometheus instance to scrape
`pushgateway.pushgateway.svc.cluster.local:9091` with matching credentials:

```yaml
scrape_configs:
  - job_name: pushgateway
    honor_labels: true
    basic_auth:
      username: batch
      password_file: /etc/prometheus/secrets/pushgateway/password
    static_configs:
      - targets: [pushgateway.pushgateway.svc.cluster.local:9091]
```

Mount the password file into Prometheus using its own secret-management
configuration. The catalog does not create that mount or change Prometheus.
`honor_labels: true` preserves the job and instance labels supplied by batch
jobs. A Prometheus Operator installation is not required for this catalog.

This setup uses HTTP inside the cluster. Restrict access to trusted jobs and
scrapers with network policy, and configure TLS in the web configuration or
an appropriate authenticated TLS endpoint if transport encryption is needed.
Do not expose the service publicly without explicitly designing those controls.

## Persistence and operation

The volume must be writable by UID/GID 65534. This is a single-replica setup
with a short interruption during updates, not a highly available service.
Do not scale multiple replicas against the same persistence file: Pushgateway
does not synchronize their in-memory state.

An accepted push acknowledges an in-memory update, not a durable transaction.
A crash can lose updates since the last completed periodic snapshot. The PVC
helps preserve completed snapshots across pod replacements; it is not a backup.

Metrics do not expire automatically. Use stable grouping keys rather than a
new key for every run, and delete the corresponding group when a job is retired.
Monitor storage, memory use, scrape health, and `push_time_seconds` for stale
jobs. Authentication and networking do not bound label cardinality or payload
size; keep callers trusted and set appropriate limits in your operating design.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
