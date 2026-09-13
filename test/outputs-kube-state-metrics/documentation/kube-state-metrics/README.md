# kube-state-metrics

This catalog installs [kube-state-metrics](https://github.com/kubernetes/kube-state-metrics) (KSM), the standard exporter that turns Kubernetes API object state into Prometheus metrics, using the prometheus-community Helm chart.

## What it deploys

- A kube-state-metrics Deployment in the `kube-state-metrics` namespace.
- Chart version `8.4.2`, which packages kube-state-metrics `v2.20.0`.
- Two replicas with [autosharding](https://github.com/kubernetes/kube-state-metrics#automated-sharding) enabled, so each replica serves a distinct shard of the object set without producing duplicate series.
- The metrics service annotated for Prometheus scraping (`prometheusScrape: true`); a Prometheus Operator `ServiceMonitor` is left disabled until a monitoring stack is selected.
- The chart's hardened default security contexts: non-root UID `65534`, read-only root filesystem, dropped capabilities, and the runtime-default seccomp profile.

The catalog installs the exporter only. Dashboards and alerting rules (for example the kubernetes-mixin set) can be layered on separately.

## Prerequisites

- The cluster must be Kubernetes `1.24` or newer.
- The cluster must be able to pull the `registry.k8s.io/kube-state-metrics/kube-state-metrics` image.
- Metrics are only useful if a scraper consumes them. This catalog does not install Prometheus or a Prometheus Operator; the `prometheus` catalog entry or any compatible scraper works.
- Cluster-wide list/watch RBAC is created automatically by the chart.

## Using the exporter

After the generated service is applied, inspect the deployment:

```sh
kubectl -n kube-state-metrics get deploy,pods,svc
```

Verify the metrics endpoint responds:

```sh
kubectl -n kube-state-metrics port-forward svc/kube-state-metrics 8080:8080
curl -s localhost:8080/metrics | head
```

If you run the prometheus-community Prometheus chart, enabling `prometheus.monitor.enabled` creates a `ServiceMonitor` for automatic scraping.
