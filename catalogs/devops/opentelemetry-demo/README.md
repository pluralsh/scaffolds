# Plural OpenTelemetry Demo

Deploys the [OpenTelemetry "Astronomy Shop" demo](https://github.com/open-telemetry/opentelemetry-demo) — a polyglot microservices application that continuously generates traces, metrics, and logs (including a built-in load generator). It's a great sandbox for demoing observability and Plural workbenches without having to instrument a real app.

This catalog is opinionated: instead of the backends the upstream chart bundles, it wires the demo into the surrounding Plural observability stack.

## Signal routing

| Signal | Destination | Catalog dependency |
|--------|-------------|--------------------|
| **Traces** | Grafana Tempo via OTLP gRPC (`tempo.tempo.svc.cluster.local:4317`) | `tempo` |
| **Metrics** | Shared OpenTelemetry Collector (`opentelemetry-collector.monitoring.svc.cluster.local:4317`), which forwards to vmagent/VictoriaMetrics | `opentelemetry`, `prometheus` |
| **Logs** | Container stdout is shipped to Elasticsearch by the Filebeat DaemonSet — no per-app wiring needed | `elastic` |

The upstream chart's bundled Jaeger, Prometheus, Grafana, and OpenSearch are disabled.

## Prerequisites

Deploy these catalogs to the target cluster first so the endpoints above exist:

1. `tempo` — the tracing backend.
2. `opentelemetry` — the shared collector that forwards metrics to VictoriaMetrics.
3. `prometheus` — VictoriaMetrics / vmagent for metrics storage.
4. `elastic` — Elasticsearch + Filebeat for logs (optional; logs still work if present).

If Tempo lives in a different namespace/cluster, update the `otlp/tempo` endpoint in `opentelemetry-demo.yaml.liquid`. Likewise for the shared collector endpoint.

## Configuration

| Parameter | Description |
|-----------|-------------|
| `cluster` | The Kubernetes cluster to deploy the demo to |
| `hostname` | DNS name to expose the demo web store under (nginx ingress + cert-manager) |

## Accessing the demo

Once deployed, the web store is available at `https://<hostname>/`. Useful sub-routes exposed through the frontend-proxy:

- `/` — the Astronomy Shop web store
- `/loadgen/` — the load generator UI
- `/feature/` — the flagd feature-flag configurator

## Verifying observability

- **Traces:** open Grafana → Tempo datasource → Explore and search for services like `frontend`, `checkout`, `cart`. The built-in load generator produces traffic automatically.
- **Metrics:** query VictoriaMetrics/Grafana for the demo's app metrics (e.g. `app_*`, service RED metrics).
- **Logs:** in Kibana, look for the demo pods' logs in the `plrl-logs*` index.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
