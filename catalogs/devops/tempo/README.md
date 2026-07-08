# Plural Grafana Tempo

This catalog deploys a single-binary [Grafana Tempo](https://grafana.com/oss/tempo/) instance to act as a distributed tracing backend. It is intended primarily as a lightweight traces store for demos (for example the OpenTelemetry demo app), but the same manifests can be scaled up for real workloads.

## What it deploys

- The `tempo` Helm chart (single binary mode) from `https://grafana.github.io/helm-charts`, into the `tempo` namespace.
- OTLP ingest enabled on:
  - gRPC: `tempo.tempo.svc.cluster.local:4317`
  - HTTP: `tempo.tempo.svc.cluster.local:4318`
- Tempo's query API on `tempo.tempo.svc.cluster.local:3100`.

## How it wires in

Anything that emits OTLP traces (an application SDK, or an OpenTelemetry Collector `otlp` exporter) can point at the gRPC/HTTP endpoints above. The OpenTelemetry demo catalog is pre-configured to export its traces here.

To view traces, add Tempo as a Grafana datasource. The `grafana` catalog exposes an optional `tempoUrl` input for exactly this — set it to the Tempo query URL that is reachable from your Grafana instance:

- **Grafana co-located on the same cluster:** `http://tempo.tempo.svc.cluster.local:3100`
- **Grafana on a different cluster (e.g. `mgmt`):** expose Tempo via an ingress and use that hostname, since in-cluster DNS won't resolve across clusters.

## Configuration

| Parameter | Description |
|-----------|-------------|
| `cluster` | The Kubernetes cluster to deploy Tempo to |

## Scaling considerations

The single-binary chart with the filesystem backend and no persistence is deliberately minimal. For production tracing you should switch to object storage (S3/GCS/Azure Blob) and consider the `tempo-distributed` chart. See the [Tempo docs](https://grafana.com/docs/tempo/latest/) for guidance.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
