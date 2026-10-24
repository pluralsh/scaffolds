# Metrics Server

This catalog installs [Metrics Server](https://github.com/kubernetes-sigs/metrics-server), the cluster-wide aggregator of resource usage metrics, using the official kubernetes-sigs Helm chart.

## What it deploys

- A Metrics Server Deployment in the `kube-system` namespace.
- Chart version `3.14.0`, which packages Metrics Server `v0.9.0`.
- Two replicas with a PodDisruptionBudget keeping at least one available during voluntary disruptions.
- The `v1beta1.metrics.k8s.io` APIService registration, which powers `kubectl top` and Horizontal Pod Autoscaler metrics lookups.
- The chart's hardened default security contexts: non-root UID `1000`, read-only root filesystem, dropped capabilities, runtime-default seccomp, and `system-cluster-critical` priority.

The catalog installs the aggregator only. It does not modify any HorizontalPodAutoscaler or resource requests on other workloads.

## Prerequisites

- The cluster must be Kubernetes `1.28` or newer.
- Nodes must expose the kubelet read-only/stats endpoints on the standard ports; this is the default on managed Kubernetes offerings.
- Skip this catalog on clusters that already run a provider-managed Metrics Server (many managed offerings ship one); check `kubectl top nodes` first.
- The cluster must be able to pull the `registry.k8s.io/metrics-server/metrics-server` image.
- On clusters where kubelets use self-signed certificates, add `--kubelet-insecure-tls` to `args`; the default configuration keeps certificate verification enabled.

## Using Metrics Server

After the generated service is applied, verify the APIService and scrape path:

```sh
kubectl get apiservice v1beta1.metrics.k8s.io
kubectl top nodes
kubectl top pods -A
```

A `kubectl top` error such as `Metrics API not available` means the APIService did not register; inspect `kubectl -n kube-system logs deploy/metrics-server` for kubelet connectivity failures.
