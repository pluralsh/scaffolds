# Qdrant vector database

This catalog deploys one Qdrant node to the selected Kubernetes cluster using the official [Qdrant Helm chart](https://qdrant.github.io/qdrant-helm). It is a small persistent datastore starter for development and application prototypes.

## What it deploys

- Qdrant chart **1.19.1**, with Qdrant image **v1.19.1**. The exact chart/app pairing is pinned because the chart manages a StatefulSet and persistent volume; major upgrades should be planned and reviewed rather than happening silently.
- One Qdrant replica with a 10 GiB `ReadWriteOnce` persistent volume.
- A `ClusterIP` service in the `qdrant` namespace. Ingress is disabled.
- API-key authentication. Plural creates a stable per-cluster key in the management cluster, passes it through `configurationRef`, and the target-cluster Helm release creates the Qdrant Secret from the rendered value.

The REST endpoint inside the target cluster is `http://qdrant.qdrant.svc.cluster.local:6333`; gRPC is available on port `6334`. Clients should supply the API key using Qdrant's `api-key` header. The generated key is managed by Plural; do not copy it into source or application manifests.

## Scope and operations

This is intentionally a single-node deployment. It does not configure Qdrant clustering or high availability. The persistent volume is the live data store; backup, snapshot export, restore testing, storage expansion, and disaster recovery remain the operator's responsibility. The chart documents StatefulSet/PVC changes that can require a controlled replacement, so keep the chart pin and storage plan under change control.

Before installing, Plural CD and its management-cluster deployment operator must already be configured. This catalog uses the existing `datastores` Catalog, the `scaffolds` source repository, the `infra` GitRepository in namespace `infra`, and an ScmConnection named `plural`. Configure that SCM connection before launching the automation. This template does not bootstrap the Plural control plane or these shared resources.

The target cluster must run Kubernetes **1.24 or newer** and have a default dynamic `ReadWriteOnce` StorageClass. The chart leaves `storageClassName` unset so it uses that cluster default; if the cluster has no suitable default provisioner, the Qdrant pod will remain pending until storage is supplied.

The API key is generated once for the cluster-scoped catalog instance and is not exposed as a setup input. Re-running the setup should preserve the `GeneratedSecret` identity and therefore the key. Rotate it as an explicit operational change, updating dependent clients at the same time.

## References

- [Qdrant Helm chart](https://qdrant.github.io/qdrant-helm)
- [Qdrant API-key authentication](https://qdrant.tech/documentation/guides/security/)
- [Qdrant distributed deployment](https://qdrant.tech/documentation/guides/distributed_deployment/)

If this catalog needs HA, larger storage, snapshots, or external access, treat that as a separate design and review rather than extending this minimal default implicitly.

## Local access

Use the target cluster context explicitly. The following Bash example reads the chart-managed `qdrant-apikey` Secret into a shell variable without printing it. It requires permission to read that Secret. The port-forward listens only on loopback.

In terminal 1:

```bash
TARGET_CONTEXT="your-target-context"
kubectl --context "$TARGET_CONTEXT" -n qdrant port-forward svc/qdrant 6333:6333
```

In terminal 2, select the same target context and query the local forward. The API key is supplied through curl's stdin configuration rather than a command-line argument:

```bash
TARGET_CONTEXT="your-target-context"
QDRANT_API_KEY="$(kubectl --context "$TARGET_CONTEXT" -n qdrant get secret qdrant-apikey -o jsonpath='{.data.api-key}' | base64 --decode)"
curl --fail --silent --show-error --config - <<EOF
url = "http://127.0.0.1:6333/collections"
header = "api-key: ${QDRANT_API_KEY}"
EOF
unset QDRANT_API_KEY TARGET_CONTEXT
```

Do not paste the key or collection response into logs, tickets, or source files. Stop the port-forward when finished.
