# Loki

This catalog installs [Loki](https://grafana.com/oss/loki/), Grafana's
log aggregation system, using the official `loki` Helm chart.

## What it deploys

- Loki chart `7.3.0` (Loki `3.x`) in `SingleBinary` deployment mode in the
  `loki` namespace.
- One single-binary replica with a `10Gi` PersistentVolumeClaim using
  filesystem-backed storage, suited for evaluation and small clusters.
- The NGINX gateway exposed as a `ClusterIP` service named `loki-gateway`,
  which fronts Loki's query and push APIs.
- The chart's `schemaConfig` (TSDB, `v13` schema) required for Loki 3.x.

Canary and chart test pods are disabled. The `read`, `write`, and `backend`
microservice replicas are scaled to zero because single-binary mode runs all
roles in one pod.

## Prerequisites

- Kubernetes `1.23` or newer with a default `StorageClass` that can provision
  a `10Gi` persistent volume.
- The cluster must be able to pull the Loki and gateway images and resolve
  the Grafana Helm repository.

## Using Loki

After the generated service is applied, inspect the components:

```sh
kubectl -n loki get pods,svc,pvc
```

Point Grafana or any log shipper at the in-cluster gateway:

```text
http://loki-gateway.loki.svc.cluster.local
```

In Grafana, add a Loki data source with that URL, then ship logs with
Promtail, Alloy, Fluent Bit, or the Grafana Kubernetes monitoring stack.

## Production notes

Filesystem storage is single-node storage: size the PVC for the retention
you want and do not scale `singleBinary` beyond one replica. For production,
switch `deploymentMode` to `SimpleScalable` or `Distributed` and point
`loki.storage.type` at S3, GCS, or Azure Blob with `schemaConfig.object_store`
matching, in a reviewed catalog update.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
