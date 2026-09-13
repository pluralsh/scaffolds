# Traefik

This catalog installs [Traefik](https://traefik.io/traefik/), the modern
reverse proxy and ingress controller, using the official `traefik` Helm
chart.

## What it deploys

- Traefik chart `v3.20.13` in the `traefik` namespace.
- A Traefik deployment exposing the proxy as a `ClusterIP` service —
  front it with your load balancer or change the service type in the
  generated values.

## Prerequisites

- Kubernetes `1.22` or newer.

## Using Traefik

After the generated service is applied, inspect the components:

```sh
kubectl -n traefik get pods,svc
```

Route traffic with standard `Ingress` or Gateway API resources; Traefik
watches them automatically.
