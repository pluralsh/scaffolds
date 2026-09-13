# Descheduler

This catalog installs the [Kubernetes
Descheduler](https://github.com/kubernetes-sigs/descheduler), which
rebalances workloads by evicting pods that violate scheduling policies,
using the official SIG-Scheduling `descheduler` Helm chart.

## What it deploys

- Descheduler chart `0.36.0` in the `descheduler` namespace, running as a
  long-lived `Deployment` (policy loop mode).

## Prerequisites

- Kubernetes `1.28` or newer.

## Using the descheduler

Policies live in the chart's `deschedulerPolicy` values — the default
profile evicts pods that fail node-affinity, spread constraints, or sit on
underutilized nodes. Inspect the deployment after applying:

```sh
kubectl -n descheduler get pods
```

See the [policy docs](https://github.com/kubernetes-sigs/descheduler#policy-default-evictor)
for the full evictor/plugin list.
