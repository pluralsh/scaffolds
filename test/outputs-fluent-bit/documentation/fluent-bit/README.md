# Fluent Bit

This catalog installs [Fluent Bit](https://fluentbit.io/), the CNCF lightweight log and metrics collector, using the official Fluent Helm chart.

## What it deploys

- A Fluent Bit DaemonSet in the `fluent-bit` namespace, collecting container logs on every node.
- Chart version `0.58.2`, which packages Fluent Bit `5.1.2`.
- The upstream default pipeline: `tail` input for container logs, `systemd` input for node journals, and the `kubernetes` filter for pod metadata enrichment.
- The built-in Prometheus metrics endpoint on port `2020`; a `ServiceMonitor` is left disabled until a monitoring stack is selected.
- A hardened container security context (privilege escalation disabled, all Linux capabilities dropped) and conservative resource requests.

The default output targets Elasticsearch at `elasticsearch-master`, matching the `elastic` entry in this catalog. Point `config.outputs` at your own sink (Loki, S3, OpenSearch, HTTP) through a values override when you use a different backend.

## Prerequisites

- Nodes must expose container logs under `/var/log/containers` and `/var/log/pods` (standard on all managed Kubernetes offerings).
- The cluster must be able to pull the `cr.fluentbit.io/fluent/fluent-bit` image.
- Fluent Bit reads node log directories, so the DaemonSet mounts host paths; the chart configures these mounts automatically.
- A log backend is required for durable shipping. Without a reachable output, Fluent Bit buffers in memory and retries.

## Using the collector

After the generated service is applied, inspect the DaemonSet:

```sh
kubectl -n fluent-bit get ds,pods
kubectl -n fluent-bit logs ds/fluent-bit --tail=50
```

Check the metrics endpoint to confirm the pipeline is flowing:

```sh
kubectl -n fluent-bit port-forward ds/fluent-bit 2020:2020
curl -s localhost:2020/api/v2/metrics/prometheus | head
```
