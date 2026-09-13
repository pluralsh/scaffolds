# Node Feature Discovery on mgmt

This installs the upstream Node Feature Discovery (NFD) chart `0.19.0` and image
`v0.19.0` in the `node-feature-discovery` namespace on the selected cluster. The
master publishes discovered node labels, workers discover features on eligible
Linux nodes, and the garbage collector removes stale feature information. Node
tainting, the topology updater and Prometheus PodMonitor creation remain disabled.
No custom NodeFeatureRule or example workload is automatically installed.

## Installation requirements

Check all namespaces for an existing NFD installation first, including NFD deployed
by NVIDIA GPU Operator or another operator. Do not install this catalog alongside
another instance that manages the same nodes and feature labels. Coordinate with
the existing owner rather than deleting or replacing its resources.

The target cluster must permit the chart's CRDs, RBAC and DaemonSet. Workers use
read-only hostPath mounts such as `/sys`, `/boot`, `/etc/os-release`, `/usr/lib`,
`/lib`, `/proc/swaps` and the NFD local feature directory. These mounts support
host feature discovery; strict Pod Security admission policies can reject them.
The catalog preserves upstream non-root containers, dropped capabilities and
read-only root filesystems. Configure cluster admission appropriately for this
system component before rollout; do not remove host mounts and expect equivalent
feature discovery.

The master requires cluster-scoped permissions to read and update Nodes, including
node metadata and status; the worker and garbage collector use the NFD APIs. This
catalog installs the upstream RBAC needed for discovery. Treat permission to create
NodeFeature and NodeFeatureRule resources as permission to influence scheduling
metadata. It does not expose a public UI or require cloud credentials.

Workers select Linux nodes. Existing taints can prevent worker scheduling because
this catalog does not add broad tolerations. Review the worker DaemonSet's desired
and ready counts against the nodes you intend to discover. Add narrowly scoped
worker tolerations in the generated Helm values if your node pools require them.

## Verify discovery and consume labels

Check that the master and garbage collector Deployments and the worker DaemonSet
are ready. Inspect NodeFeature objects and actual node labels before configuring
workload placement:

```sh
kubectl -n node-feature-discovery get deployments,daemonsets,pods
kubectl -n node-feature-discovery get nodefeatures.nfd.k8s-sigs.io
kubectl get nodes --show-labels
```

Use a discovered `feature.node.kubernetes.io/` label and its observed value in your
workload's `nodeSelector` or node affinity. Feature availability depends on the
underlying hardware and operating system; do not assume a CPU instruction or device
exists on every cluster. A selector with no matching nodes leaves Pods Pending.
Labels describe capabilities; they do not install GPU drivers or device plugins.

## Upgrades and removal

The chart includes the NFD API CRDs. Confirm they are installed before the controllers
start, and review the upstream CRD upgrade procedure when changing versions. CRD
removal deletes associated custom resources and can affect other consumers.

`postDeleteCleanup` is explicitly disabled: a Helm post-delete hook is not a normal
GitOps application resource. Removing this Plural service does not promise Helm's
uninstall behavior or automatic cleanup of node labels, annotations or extended
resources. Before removal, migrate workloads that rely on the labels, stop the NFD
controllers, and plan scoped cleanup using the upstream uninstall procedure. Review
remaining CRDs and custom resources separately; do not delete shared APIs blindly.

Generation and Helm rendering validate configuration, not discovery on your hardware
or scheduling in your cluster. See the upstream
[Helm deployment and upgrade guide](https://kubernetes-sigs.github.io/node-feature-discovery/v0.19/deployment/helm.html),
[uninstallation guide](https://kubernetes-sigs.github.io/node-feature-discovery/v0.19/deployment/uninstallation.html), and
[pinned chart values](https://github.com/kubernetes-sigs/node-feature-discovery/blob/v0.19.0/deployment/helm/node-feature-discovery/values.yaml).
