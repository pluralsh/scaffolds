# Headlamp

This catalog installs [Headlamp](https://headlamp.dev/), the extensible
Kubernetes web UI from the Kubernetes SIG UI project, using the official
`headlamp` Helm chart.

## What it deploys

- Headlamp chart `0.45.0` in the `headlamp` namespace.
- The Headlamp web UI deployment exposed as a `ClusterIP` service.
- Ingress and persistent storage are disabled by default; enable them in
  the generated service values when needed.

## Prerequisites

- Kubernetes `1.23` or newer.
- For authentication Headlamp uses a Kubernetes service account token;
  see the [access docs](https://headlamp.dev/docs/latest/installation/).

## Using Headlamp

After the generated service is applied, inspect the components:

```sh
kubectl -n headlamp get pods,svc
```

Forward the service and log in with a cluster service account token:

```sh
kubectl -n headlamp port-forward svc/headlamp 8080:80
```
