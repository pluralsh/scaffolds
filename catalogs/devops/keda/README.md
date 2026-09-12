# KEDA

This catalog installs [KEDA](https://keda.sh/), the Kubernetes-based event-driven
autoscaler, using the official KEDA Helm chart.

## What it deploys

- KEDA chart `2.20.2`, which packages KEDA `2.20.2`.
- The KEDA operator, metrics API server, and admission webhooks in the `keda`
  namespace.
- KEDA CRDs and the cluster-scoped RBAC required to watch workloads and
  `ScaledObject`, `ScaledJob`, `TriggerAuthentication`, and related resources.
- Two replicas for each serving component, using KEDA's leader election for
  controller availability and a ClusterIP service for the metrics API.
- Secure upstream container defaults: non-root pods, dropped Linux
  capabilities, disabled privilege escalation, read-only root filesystems, and
  the runtime-default seccomp profile.

The chart watches all namespaces by default, so application teams can create
KEDA resources anywhere in the cluster. It does not create a `ScaledObject`,
configure an external scaler, or enable cloud-provider workload identity.

## Prerequisites

- Kubernetes `1.23` or newer with permission to install CRDs, cluster-scoped
  RBAC, an aggregated metrics API service, and admission webhooks.
- The cluster must be able to pull the KEDA images from `ghcr.io` and resolve
  the official Helm repository.
- Workloads that use KEDA still need an appropriate `ScaledObject`, trigger
  authentication, and access to the selected event source. Keep credentials in
  Kubernetes Secrets or an external secret manager rather than in Git.

## Using KEDA

After the generated service is applied, inspect the components and CRDs:

```sh
kubectl -n keda get deploy,pods,svc
kubectl get crd scaledobjects.keda.sh scaledjobs.keda.sh triggerauthentications.keda.sh
```

Create a `ScaledObject` or `ScaledJob` for a workload and choose a trigger from
the upstream [KEDA scaler catalog](https://keda.sh/docs/latest/scalers/). Use
`TriggerAuthentication` or `ClusterTriggerAuthentication` for secrets and
review the minimum/maximum replica bounds before enabling autoscaling in
production.

KEDA's metrics API service is kept internal and no ingress or dashboard is
created by this catalog. If Prometheus metrics are needed, enable the chart's
service-monitor settings in a reviewed catalog update and ensure a compatible
Prometheus Operator is already installed.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
