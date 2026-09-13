# Spegel

This catalog installs [Spegel](https://spegel.dev/), a stateless cluster-local OCI registry mirror that lets Kubernetes nodes serve cached container image layers to one another before falling back to the upstream registry.

## What it deploys

- The official Spegel Helm chart `0.7.4` from `oci://ghcr.io/spegel-org/helm-charts`.
- A Spegel DaemonSet in the `spegel` namespace on the selected cluster.
- The upstream registry/router/bootstrap services and node-local registry mirror configuration.
- Persistent node identity under `/var/lib/spegel` using the chart's upstream defaults.

Monitoring integrations remain opt-in so the catalog does not require Prometheus Operator or Grafana CRDs.

## Prerequisites and compatibility

Spegel only supports **Containerd**. Before deploying, check the upstream [compatibility guide](https://spegel.dev/docs/getting-started/#compatibility), because some Kubernetes distributions require node-level Containerd configuration that this catalog intentionally does not modify.

Important upstream guidance includes:

- AKS and several other distributions work with the default chart configuration.
- EKS commonly requires Containerd configuration changes (for example `config_path` and `discard_unpacked_layers`) before installing Spegel.
- K3s and RKE2 ship their own Spegel integration; use the distribution documentation instead of this catalog.
- GKE is currently marked unsupported by Spegel upstream.
- Spegel follows Kubernetes N-2 compatibility and supports maintained Containerd versions from 1.7 onward according to the upstream release policy.

Because these settings live on Kubernetes nodes and can require a Containerd restart, validate the selected cluster before merging the generated deployment PR.

## Deploying

Choose the target cluster in the catalog form. The generated PR creates a `ServiceDeployment` for the official Spegel chart and a small values file that keeps monitoring CRDs disabled by default.

After deployment, check the DaemonSet and services:

```sh
kubectl -n spegel get daemonset,pods,svc
```

Spegel falls back to the upstream registry when a layer is unavailable in the cluster, so a healthy-looking pod is not by itself proof that peer-to-peer pulls are occurring. Follow the upstream [verification procedure](https://spegel.dev/docs/getting-started/#verify-deployment) to schedule the same image on different nodes and confirm that the second node can pull cached content from a peer.

## Security note

In multi-tenant clusters, cached private images can change the assumptions around registry credential checks. Review Spegel's [multi-tenant credentials guidance](https://spegel.dev/docs/usage/multi-tenant-credentials/) and your cluster's admission policy before enabling it for workloads that use private registries.

## Contributing

If there are features or documentation you'd like to add to this setup, please contribute them back at https://github.com/pluralsh/scaffolds.
