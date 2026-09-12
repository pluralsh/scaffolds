# VictoriaMetrics

This catalog installs [VictoriaMetrics](https://victoriametrics.com/), the
fast, cost-efficient metrics backend, using the official
`victoria-metrics-single` Helm chart.

## What it deploys

- `victoria-metrics-single` chart `0.46.0`, which packages the single-node
  VictoriaMetrics server in the `victoria-metrics` namespace.
- One server replica with a `16Gi` PersistentVolumeClaim and a retention
  period of three months (`retentionPeriod: "3"`), suited for evaluation and
  small clusters.
- A `ClusterIP` service exposing the metrics API on port `8428`; no ingress
  is created by this catalog.
- A dedicated ServiceAccount for the server.

Single-node mode handles metric ingestion, storage, and querying in one pod.
It is a drop-in Prometheus-compatible backend for remote write and PromQL.

## Prerequisites

- Kubernetes `1.23` or newer with a default `StorageClass` that can provision
  a `16Gi` persistent volume.
- The cluster must be able to pull the VictoriaMetrics image and resolve the
  VictoriaMetrics Helm repository.

## Using VictoriaMetrics

After the generated service is applied, inspect the components:

```sh
kubectl -n victoria-metrics get pods,svc,pvc
```

Remote-write endpoint for Prometheus, OpenTelemetry Collector, or Grafana
Agent / Alloy:

```text
http://victoria-metrics-server.victoria-metrics.svc.cluster.local:8428/api/v1/write
```

Query endpoint for Grafana data sources (Prometheus-compatible):

```text
http://victoria-metrics-server.victoria-metrics.svc.cluster.local:8428
```

The built-in VMUI is reachable at `http://<service>:8428/vmui` for ad-hoc
queries once connected inside the cluster.

## Production notes

Single-node mode does not scale horizontally; size the PVC and
`retentionPeriod` for the expected metric volume. For production, switch to
the `victoria-metrics-k8s-stack` or `victoria-metrics-cluster` charts in a
reviewed catalog update, and front the API with authenticated ingress.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
