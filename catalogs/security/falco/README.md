# Falco

This catalog installs [Falco](https://falco.org/), the CNCF-graduated runtime threat detection engine for Kubernetes, using the official Falco Helm chart.

## What it deploys

- A Falco sensor DaemonSet in the `falco` namespace, covering every schedulable node.
- Chart version `9.1.0`, which packages Falco `0.44.1`.
- The modern eBPF probe (`driver.kind: modern_ebpf`), so no kernel module build or driver-loader init container is required on supported kernels.
- The k8s-metacollector component for Kubernetes metadata enrichment (pod names, namespaces, and labels attached to alerts).
- Container-engine metadata collection across Docker, containerd, CRI-O, podman, and related runtimes.
- Baseline resource requests and limits for predictable per-node usage.

Falco Sidekick and Falco Talon are left disabled: alert forwarding channels (Slack, SIEM, webhook) and automated response actions are deployment-specific and can be enabled later through values overrides.

## Prerequisites

- Nodes must run a Linux kernel recent enough for the modern eBPF probe (kernel `5.8` or newer, with BTF enabled; all current EKS, GKE, and AKS default node images qualify).
- The cluster must be able to pull the `docker.io/falcosecurity/falco` image and the `ghcr.io/falcosecurity/plugins` plugin images.
- Falco requires privileged access to the node kernel for syscall capture; the chart requests this automatically.
- No alert sink is configured by default. Enable `falcosidekick` or ship `falco` logs to a log pipeline to route alerts.

## Using the sensor

After the generated service is applied, inspect the DaemonSet:

```sh
kubectl -n falco get ds,pods
kubectl -n falco logs ds/falco -c falco --tail=50
```

Trigger a test alert by opening a shell in a pod (the default ruleset flags `Terminal shell in container`):

```sh
kubectl -n falco exec -it deploy/falco-k8s-metacollector -- /bin/sh
```

Then check Falco logs for the emitted rule violation.
