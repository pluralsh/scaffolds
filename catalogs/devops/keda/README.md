# KEDA

Deploys [KEDA](https://keda.sh/) as a cluster-wide event-driven autoscaling controller using the official Helm chart.

KEDA extends Kubernetes autoscaling with `ScaledObject` and `ScaledJob` custom resources and exposes external metrics to the Horizontal Pod Autoscaler. The chart installs the operator, metrics API server, admission webhooks, and required CRDs in the `keda` namespace.

## After installation

Create a `ScaledObject` or `ScaledJob` in an application namespace and configure one of KEDA's supported event sources. Authentication for external systems should be supplied through Kubernetes secrets and KEDA `TriggerAuthentication` or `ClusterTriggerAuthentication` resources rather than embedded in the catalog deployment.

## Upstream

- Documentation: https://keda.sh/docs/
- Helm repository: https://kedacore.github.io/charts
- Source: https://github.com/kedacore/keda
