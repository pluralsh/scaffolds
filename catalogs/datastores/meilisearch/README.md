# Meilisearch

This catalog deploys one authenticated Meilisearch instance in production mode to the selected Kubernetes cluster using the official [Meilisearch Helm chart](https://github.com/meilisearch/meilisearch-kubernetes/tree/main/charts/meilisearch). It is a small internal search service starter for applications that need persistent indexing without a multi-node topology.

## What it deploys

- Official Meilisearch Helm chart **0.38.0** with the matching `getmeili/meilisearch:v1.53.1` image. The chart and image are pinned so upgrades to the search engine and storage behavior are deliberate.
- One Meilisearch StatefulSet replica in production mode (`MEILI_ENV=production`) with analytics disabled.
- A `ClusterIP` service named `meilisearch` in the `meilisearch` namespace on port `7700`; ingress and external service exposure are disabled.
- A dynamically provisioned 10 GiB `ReadWriteOnce` volume mounted at `/meili_data`; the chart's `helm.sh/resource-policy: keep` annotation leaves the claim available for recovery after an uninstall.
- A cluster-specific Plural `GeneratedSecret` transported through `configurationRef`. A small wrapper chart creates the target-cluster Secret `meilisearch-master-key` with the chart-required `MEILI_MASTER_KEY` key, and the official chart dependency consumes it with `auth.existingMasterKeySecret`. The master key is absent from the rendered ConfigMap and from literal Pod environment values.

The in-cluster endpoint is `http://meilisearch.meilisearch.svc.cluster.local:7700`. Requests that need authentication should send the generated master key as a Bearer token. Use the key only through a client credential store or protected runtime variable; do not copy it into application manifests or source files.

## Prerequisites and limits

Plural CD and its management-cluster deployment operator must already be configured. This catalog uses the existing `datastores` Catalog, the `scaffolds` source repository, the `infra` GitRepository in namespace `infra`, and an ScmConnection named `plural`. Configure that SCM connection before launching the automation. This template does not bootstrap those shared resources.

The target cluster must run Kubernetes **1.24 or newer** and have a default dynamic `ReadWriteOnce` StorageClass. The values leave `persistence.storageClass` unset so Kubernetes selects that default provisioner; without one, the Meilisearch pod remains pending until storage is supplied. The catalog contract fixtures can be rendered locally without a cluster.

This is intentionally a single-node deployment. It does not configure replication, sharding, or high availability. The persistent volume is the live index store; its keep policy prevents an intentional Helm uninstall from deleting the claim, while backups, snapshot export, restore testing, capacity planning, and disaster recovery remain the operator's responsibility. Treat chart, image, master-key, and PVC changes as planned changes.

The management `GeneratedSecret` is named `meilisearch-master-key-<cluster>` and contains a `master_key` value generated once per cluster. `configurationRef` transports it to the target render, where the wrapper chart writes `MEILI_MASTER_KEY` into the target-cluster `meilisearch-master-key` Secret. The wrapper pins the official chart dependency in `Chart.lock` and bundles the exact [upstream `meilisearch-0.38.0.tgz` release asset](https://github.com/meilisearch/meilisearch-kubernetes/releases/download/meilisearch-0.38.0/meilisearch-0.38.0.tgz) at `helm/charts/meilisearch-0.38.0.tgz`, so the default Helm agent does not need dependency downloads or a global dependency-update flag. Its SHA-256 is `2A162D6694C8C21AFC1BEF7A8EA239E0F1F628AD615E5C137DC668F3C8B3BAA8`; the upstream MIT license is included at `helm/LICENSE`. Re-running the setup preserves the generated secret identity and therefore the key. Rotate the key as an explicit change coordinated with clients, update dependent API keys as required, and restart the Meilisearch StatefulSet after the target Secret changes because the chart reads it through `envFrom` when the Pod starts.

The client connection is HTTP in this minimal internal deployment. Keep the `ClusterIP` service inside a trusted cluster network; add TLS and any external access as a separate reviewed change before exposing Meilisearch to an untrusted network.

## Local access

Use the target cluster context explicitly. The following port-forward listens only on loopback:

```bash
TARGET_CONTEXT="your-target-context"
kubectl --context "$TARGET_CONTEXT" -n meilisearch port-forward svc/meilisearch 7700:7700
```

Retrieve the chart-managed target Secret with the same target context and query the authenticated API without printing the key:

```bash
MEILI_MASTER_KEY="$(kubectl --context "$TARGET_CONTEXT" -n meilisearch get secret meilisearch-master-key -o jsonpath='{.data.MEILI_MASTER_KEY}' | base64 --decode)"
curl --fail --silent --show-error --config - <<EOF
url = "http://127.0.0.1:7700/indexes"
header = "Authorization: Bearer ${MEILI_MASTER_KEY}"
EOF
unset MEILI_MASTER_KEY TARGET_CONTEXT
```

Do not paste the key or response into logs, tickets, or source files. Stop the port-forward when finished.

## References

- [Meilisearch Kubernetes chart](https://github.com/meilisearch/meilisearch-kubernetes/tree/main/charts/meilisearch)
- [Meilisearch Kubernetes MIT license](https://github.com/meilisearch/meilisearch-kubernetes/blob/main/LICENSE)
- [Meilisearch configuration](https://www.meilisearch.com/docs/learn/configuration/instance_options)
- [Meilisearch authentication](https://www.meilisearch.com/docs/learn/security/basic_security)
- [Meilisearch production deployment](https://www.meilisearch.com/docs/learn/self_hosted/configure_meilisearch)

If this catalog needs high availability, larger storage, TLS, external access, or a different backup policy, treat that as a separate design and review rather than extending this minimal default implicitly.
