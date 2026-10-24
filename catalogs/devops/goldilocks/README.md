# Goldilocks

This is a baseline, prod-ready Goldilocks installation using Plural.

Goldilocks is a Fairwinds tool that inspects what your workloads actually ask for
versus what they really use, and reports a recommended CPU and memory request
per container. It publishes those numbers by creating
[VerticalPodAutoscaler](https://github.com/kubernetes/autoscaler/tree/master/vertical-pod-autoscaler)
objects in `Off` mode, so nothing is resized unless you choose to act on a
recommendation.

## What this setup deploys

- The upstream [Goldilocks chart](https://github.com/FairwindsOps/charts/tree/master/stable/goldilocks)
  pinned to a specific chart version, into a dedicated `goldilocks` namespace.
- The controller, which watches workloads and maintains the VPA objects.
- The dashboard, which surfaces the recommendations in a web UI.
- The VPA sub-chart, because Goldilocks has nothing to write recommendations into
  without it.

Both the controller and the dashboard are set to `on-by-default: true`, so every
namespace is monitored without having to label namespaces individually. If you
would rather opt namespaces in one at a time, remove those flags and label the
namespaces you care about with `goldilocks.fairwinds.com/enabled=true`.

## Prerequisites

- Kubernetes 1.22 or newer, which is the chart's own minimum.
- No pre-existing VerticalPodAutoscaler installation. VPA installs cluster-scoped
  CRDs, so running two copies will conflict. If you already run VPA, set
  `vpa.enabled: false` in `goldilocks.yaml` and let Goldilocks reuse it.
- `metrics-server` (or another Metrics API implementation) if you want the
  recommendations to reflect live usage rather than requests alone. The chart can
  install it for you, but this setup leaves it disabled because most clusters
  already provide it.

## Reading the output

The dashboard is a `ClusterIP` service, so reach it through the Plural service
proxy or your own `kubectl port-forward` rather than exposing it publicly:

```bash
kubectl -n goldilocks port-forward svc/goldilocks-dashboard 8080:80
```

Recommendations appear as soon as the controller has observed enough usage data,
which typically takes a few hours of real traffic. VPA runs in `Off` mode, so
applying a recommendation is a deliberate change you make yourself.

## Contributing

If there are any features or documentation you'd like to add to this setup, please
feel free to contribute back at https://github.com/pluralsh/scaffolds.
