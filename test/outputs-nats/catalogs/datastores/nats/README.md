# NATS JetStream

This catalog deploys one authenticated NATS server with JetStream to the selected Kubernetes cluster using the official [NATS Helm chart](https://github.com/nats-io/k8s/tree/main/helm/charts/nats). It is a small internal messaging and streaming starter for applications that need durable subjects without a multi-node cluster.

## What it deploys

- Official NATS Helm chart **2.14.6** with the matching `nats:2.14.6-alpine` image. The chart and image are pinned so upgrades to the NATS server and JetStream storage behavior are deliberate.
- One NATS StatefulSet replica with JetStream file storage at `/data` and a dynamically provisioned 10 GiB `ReadWriteOnce` volume.
- A `ClusterIP` client service named `nats` in the `nats` namespace on the standard NATS client port `4222`. Cluster routes, leaf nodes, WebSocket, MQTT, ingress, and the optional NATS Box are disabled.
- Username/password authentication from a cluster-specific Plural `GeneratedSecret` through `configurationRef`. The official chart creates an in-namespace `Opaque` Secret named `nats-auth` from those rendered values. The NATS container reads its `username` and `password` through `container.env.valueFrom.secretKeyRef`, while `config.merge.authorization.users` references the environment variables; credentials do not appear in the rendered ConfigMap or as literal Pod env values, and no credential is committed to Git.

The in-cluster client endpoint is `nats://nats.nats.svc.cluster.local:4222`. The generated username is `nats`; the password is managed by Plural. Use a NATS client that supplies both credentials and do not copy the password into application manifests or source files.

## Prerequisites and limits

Plural CD and its management-cluster deployment operator must already be configured. This catalog uses the existing `datastores` Catalog, the `scaffolds` source repository, the `infra` GitRepository in namespace `infra`, and an ScmConnection named `plural`. Configure that SCM connection before launching the automation. This template does not bootstrap those shared resources.

The target cluster must run Kubernetes **1.24 or newer** and have a default dynamic `ReadWriteOnce` StorageClass. The values leave `storageClassName` empty so Kubernetes selects that default provisioner; without one, the NATS pod remains pending until storage is supplied. The catalog contract fixtures can be rendered locally without a cluster.

This is intentionally a single-node deployment. It does not configure NATS clustering, replication, or high availability. The persistent volume is the live JetStream store; stream retention, backups, snapshot export, restore testing, storage expansion, and disaster recovery remain the operator's responsibility. Treat chart, image, authentication, and PVC changes as planned changes.

The management `GeneratedSecret` is named `nats-users-<cluster>` and contains `username` and `password` keys. Generated passwords begin with the fixed alphabetic prefix `nats_` followed by 48 random alphanumeric characters so NATS cannot parse a digit-leading value as a numeric or unit token. `configurationRef` transports those values to the target Helm render as `configuration.username` and `configuration.password`; the official chart writes them to the target-cluster `nats-auth` Secret. Re-running the setup preserves the generated secret identity and therefore the password. Rotate it as an explicit change coordinated with clients, and use the same string-safe alphabetic prefix for manual values before restarting the NATS StatefulSet after the Secret changes because environment variables are read when the Pod starts.

TLS is disabled in this minimal internal deployment. NATS username/password authentication therefore crosses the internal client connection in plaintext at the protocol level; keep the `ClusterIP` service inside a trusted cluster network. Add TLS as a separate reviewed change before exposing NATS to an untrusted network.

## Local access

Use the target cluster context explicitly. The following port-forward listens only on loopback:

```bash
TARGET_CONTEXT="your-target-context"
kubectl --context "$TARGET_CONTEXT" -n nats port-forward svc/nats 4222:4222
```

Retrieve the chart-managed target Secret with the same target context, then configure a local NATS CLI context or client with username `nats`, that password, and `nats://127.0.0.1:4222`:

```bash
NATS_PASSWORD="$(kubectl --context "$TARGET_CONTEXT" -n nats get secret nats-auth -o jsonpath='{.data.password}' | base64 --decode)"
```

Keep the password in a shell variable or client credential store and do not print it in logs. Stop the port-forward when finished.

## References

- [NATS Helm chart](https://github.com/nats-io/k8s/tree/main/helm/charts/nats)
- [NATS Helm repository](https://nats-io.github.io/k8s/)
- [NATS authentication](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro)
- [NATS JetStream](https://docs.nats.io/running-a-nats-service/configuration#jetstream)

If this catalog needs a clustered topology, external access, TLS, larger storage, or a different retention and backup policy, treat that as a separate design and review rather than extending this minimal default implicitly.
