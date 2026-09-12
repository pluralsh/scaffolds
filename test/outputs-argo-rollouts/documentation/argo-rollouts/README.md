# Argo Rollouts

This catalog installs [Argo Rollouts](https://argoproj.github.io/argo-rollouts/), a Kubernetes controller for progressive delivery, using the official Argo Helm chart.

## What it deploys

- The Argo Rollouts controller in the `argo-rollouts` namespace.
- Chart version `2.43.1`, which packages Argo Rollouts `v1.10.0`.
- The Rollout, AnalysisRun, Experiment, and related CRDs, kept when the Helm release is removed.
- Two controller replicas with health checks and metrics exposed through an internal ClusterIP service.
- Hardened non-root pod and container security contexts, including dropped Linux capabilities and the runtime-default seccomp profile.
- No dashboard, ingress, or external route by default.

The catalog installs the controller and CRDs but does not change any existing Deployment into a Rollout. Teams can introduce `Rollout`, `AnalysisTemplate`, and `Experiment` resources separately and choose a traffic-routing provider explicitly.

## Prerequisites

- The cluster must be Kubernetes `1.21` or newer and permit installation of cluster-scoped CRDs, RBAC, and webhooks as applicable.
- The cluster must be able to pull the official chart and `quay.io/argoproj/argo-rollouts` image.
- A rollout requires an ingress or service-mesh integration supported by Argo Rollouts if it will shift traffic. This catalog leaves provider-specific RBAC disabled until that integration is selected.
- Metrics are exposed by the controller, but this catalog does not install Prometheus or a Prometheus Operator.

## Using the controller

After the generated service is applied, inspect the controller and CRDs:

```sh
kubectl -n argo-rollouts get deploy,pods,svc
kubectl get crd rollouts.argoproj.io analysisruns.argoproj.io experiments.argoproj.io
```

To try a rollout, create a `Rollout` resource and an appropriate `AnalysisTemplate`, then follow the upstream [progressive delivery guide](https://argoproj.github.io/argo-rollouts/getting-started/). Do not apply a production traffic-routing strategy until its controller integration and RBAC have been reviewed.

The optional dashboard is intentionally disabled. If it is needed, enable it in the generated catalog values and expose it only through an explicitly secured internal route or port-forward.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
