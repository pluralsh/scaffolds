# Prometheus Blackbox Exporter

This catalog deploys the [Prometheus Blackbox Exporter](https://github.com/prometheus/blackbox_exporter) with the official chart from the Prometheus Community Helm repository.

## What it deploys

- One Blackbox Exporter replica in the `blackbox-exporter` namespace.
- The upstream `prometheus-blackbox-exporter` chart, pinned to version `11.18.0` (Blackbox Exporter `v0.28.0`).
- An internal `ClusterIP` service on port `9115`.
- The chart's standard `http_2xx` probe module, which follows redirects and prefers IPv4.
- A read-only, non-root container with privilege escalation disabled and all Linux capabilities dropped.
- No automatically mounted Kubernetes service-account token and no Ingress or Gateway route.

The exporter does not probe targets by itself. A Prometheus-compatible scraper calls its `/probe` endpoint with a target URL and module. Configure scrape jobs or the chart's optional `ServiceMonitor.targets` in the generated values file after deployment.

## Prerequisites

- The target cluster must be Kubernetes `1.21` or newer and able to pull the upstream chart and image.
- The Plural SCM connection named `plural` and the `infra` GitRepository must be available for the generated ServiceDeployment.
- A Prometheus-compatible scraper is required to collect probe results. This catalog does not install Prometheus or a Prometheus Operator.
- The default configuration performs HTTP probes only. ICMP probing requires an explicit security review and additional network capability; this catalog does not enable it.

## Using the exporter

The automation writes a ServiceDeployment under `bootstrap/apps/blackbox-exporter/<cluster>/` and the chart values under `helm/blackbox-exporter/<cluster>/`.

For a direct smoke check from inside the cluster, query the service with a target and module:

```sh
kubectl -n blackbox-exporter port-forward service/blackbox-exporter 9115:9115
curl 'http://127.0.0.1:9115/probe?target=https%3A%2F%2Fexample.org&module=http_2xx'
```

Do not put credentials or bearer tokens in a target URL. Use the chart's supported Secret and TLS configuration for authenticated probes, and keep those values out of Git.

If a Prometheus Operator is already installed, set `serviceMonitor.enabled` and add target entries in the generated values file. The chart creates a ServiceMonitor per target and uses the configured module and interval.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
