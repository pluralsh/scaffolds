# Plural Telemetry agent

This catalog deploys [Grafana Alloy](https://grafana.com/oss/alloy-opentelemetry-collector/) on every cluster managed by Plural and ships metrics, logs and traces to a Plural Telemetry instance. Install the `plural-telemetry` catalog first.

## What it deploys

Two global services, applied to all clusters including the management cluster:

- `alloy-operator` (Helm chart `alloy-operator` 0.8.0) into the `alloy` namespace, with the Alloy CRDs.
- `alloy` (Helm chart `k8s-monitoring` 4.5.3) into the `alloy` namespace. It collects:
  - cluster, host (node-exporter) and kube-state metrics, plus ServiceMonitors and PodMonitors when the Prometheus operator CRDs exist;
  - pod logs and Kubernetes events;
  - OTLP metrics, logs and traces pushed by applications, on the Alloy OTLP receivers (gRPC and HTTP).

Everything is sent to the Telemetry ingress, in the `default` telemetry namespace:

- Metrics: `https://<hostname>/metrics/write/ns/default/api/v1/write`
- Logs: `https://<hostname>/logs/write/ns/default/loki/api/v1/push`
- Traces: `https://<hostname>/traces/write/ns/default`

Metrics and logs carry the Plural cluster handle as the `cluster` label; on traces it is set by the `k8s-monitoring` chart as a resource attribute, so filter traces on that attribute rather than a label.

## Credentials

The agents reuse the basic auth credentials the `plural-telemetry` catalog generates on the management cluster (secret `telemetry-auth-<telemetryCluster>` in the `apps` namespace). The `alloy` global service reads them through `configurationRef`, so no password is stored in git.

## Configuration

- `telemetryCluster`: handle of the cluster Plural Telemetry is deployed to. It selects the credentials secret.
- `hostname`: DNS name Plural Telemetry is served under, as set in the `plural-telemetry` catalog.

## Notes

- Both services are global and named `alloy-operator` and `alloy`. If you already run services with those names (for example a hand-written Alloy setup), remove them first or the two will conflict.
- The credentials allow reading as well as writing. A cluster that runs this agent can query all telemetry in the `default` namespace.
- Collectors are sized by `k8s-monitoring` defaults. For large clusters, tune the `collectors` section of `helm/telemetry-agent/alloy.yaml.liquid` in your infra repo.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
