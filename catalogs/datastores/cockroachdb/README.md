# CockroachDB

This catalog installs [CockroachDB](https://www.cockroachlabs.com/), the
distributed SQL database, using the official `cockroachdb` Helm chart.

## What it deploys

- CockroachDB chart `22.0.3` in the `cockroachdb` namespace.
- A three-replica StatefulSet with `10Gi` persistent volumes per node.

## Prerequisites

- Kubernetes `1.23` or newer with a default `StorageClass` able to
  provision three `10Gi` volumes.

## Using CockroachDB

After the generated service is applied, inspect the cluster:

```sh
kubectl -n cockroachdb get pods,svc,pvc
```

Connect with the built-in SQL client:

```sh
kubectl -n cockroachdb exec -it cockroachdb-0 -- ./cockroach sql --certs-dir=/cockroach/cockroach-certs
```
