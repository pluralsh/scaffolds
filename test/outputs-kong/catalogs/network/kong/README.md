# Kong Gateway

This catalog installs [Kong Gateway](https://konghq.com/) together with the
Kong Ingress Controller using the official `kong` Helm chart.

## What it deploys

- Kong chart `11.9.13` in the `kong` namespace.
- Kong Gateway running in DB-less mode (`database: "off"`), configured
  declaratively through Kubernetes resources.
- The Kong Ingress Controller, watching `Ingress` and Gateway API
  resources.
- The proxy exposed as a `ClusterIP` service; attach your own load
  balancer or ingress as needed.

## Prerequisites

- Kubernetes `1.23` or newer.
- `KongIngress` / `TCPIngress` CRDs are managed by your cluster tooling
  (`ingressController.installCRDs` is disabled to avoid clashes with
  existing installations).

## Using Kong

After the generated service is applied, inspect the components:

```sh
kubectl -n kong get pods,svc
```

Route traffic by creating `Ingress` or `HTTPRoute` resources; the ingress
controller translates them into Kong configuration automatically.
