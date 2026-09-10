# Plural Grafana Tempo

This catalog deploys a single-binary [Grafana Tempo](https://grafana.com/oss/tempo/) instance backed by the target cloud's object storage.

## What it deploys

- The `tempo` Helm chart (single binary mode) from `https://grafana-community.github.io/helm-charts`, into the `tempo` namespace.
- An S3 bucket, GCS bucket, or private Azure Blob container for trace blocks.
- Passwordless workload credentials:
  - EKS Pod Identity on AWS.
  - Azure Workload Identity on AKS.
  - GKE Workload Identity on GCP.
- A small persistent volume for Tempo's write-ahead log and in-flight traces.
- OTLP ingest enabled on:
  - gRPC: `tempo.tempo.svc.cluster.local:4317`
  - HTTP: `tempo.tempo.svc.cluster.local:4318`
- Tempo's query API on `tempo.tempo.svc.cluster.local:3200`.

## How it wires in

Anything that emits OTLP traces (an application SDK, or an OpenTelemetry Collector `otlp` exporter) can point at the gRPC/HTTP endpoints above. The OpenTelemetry demo catalog is pre-configured to export its traces here.

To view traces, add Tempo as a Grafana datasource. The `grafana` catalog exposes an optional `tempoUrl` input for exactly this — set it to the Tempo query URL that is reachable from your Grafana instance:

- **Grafana co-located on the same cluster:** `http://tempo.tempo.svc.cluster.local:3200`
- **Grafana on a different cluster (e.g. `mgmt`):** expose Tempo via an ingress and use that hostname, since in-cluster DNS won't resolve across clusters.

## Configuration

- `cluster`: Kubernetes cluster to deploy Tempo to.
- `cloud`: `aws`, `azure`, or `gcp`.
- `bucket`: globally unique bucket name on AWS/GCP, or the Azure Blob container name.
- `region`: AWS region for the target EKS cluster and S3 bucket (AWS only).
- `storageAccount`: existing storage account in the target AKS cluster's resource group (Azure only).

Terraform provisions the object storage and cloud identity first. The Tempo service imports those stack outputs into its Helm values, so bucket names and identity metadata are not duplicated or stored as static credentials.

## Scaling considerations

This scaffold uses object storage, but the single Tempo replica is still aimed at smaller installations and demos. For high availability or larger production workloads, use the `tempo-distributed` chart. See the [Tempo docs](https://grafana.com/docs/tempo/latest/) for guidance.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
