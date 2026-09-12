# Valkey standalone datastore

This catalog deploys one authenticated Valkey instance to the selected Kubernetes cluster using the official [Valkey Helm chart](https://github.com/valkey-io/valkey-helm). It is a small internal datastore starter for applications that need Redis-compatible commands.

## What it deploys

- Valkey Helm chart **0.12.0** with the matching `valkey/valkey:9.1.2` image. The chart and image are pinned together so persistence changes are deliberate rather than an automatic major upgrade.
- One standalone Valkey pod, with replicas disabled.
- The supported `Recreate` deployment strategy, so an upgrade does not try to overlap two writers on the single RWO data volume. Expect a short write outage while the replacement pod starts.
- A `ClusterIP` service named `valkey-<cluster>` on port `6379`; no external listener or ingress is configured.
- A dynamically provisioned 10 GiB `ReadWriteOnce` volume. `keepPvc` is enabled so an uninstall does not intentionally remove the data claim.
- ACL authentication enabled with the required `default` user. The password comes from a cluster-specific Plural `GeneratedSecret` through `configurationRef`; the chart creates its target Secret (`valkey-<cluster>-auth`, key `default-password`) during the Helm render. The password is never committed to Git or placed in the values fixture.
- Authenticated readiness requires an actual `PONG` response. It reads the generated credential from a read-only Secret mount, so an unavailable or loading server is not advertised as ready merely because the client command ran.

## Prerequisites and limits

The target cluster must run Kubernetes **1.24 or newer** and have a default dynamic `ReadWriteOnce` StorageClass. The values leave `dataStorage.className` empty so Kubernetes selects that default provisioner; without one, the Valkey pod remains pending. The Plural installation needs the normal `PrAutomation`, `GeneratedSecret`, and remote `ServiceDeployment` controllers, plus the `plural` SCM connection used by the surrounding catalogs. Local contract checks use the pinned Plural CLI `0.11.0`.

This is deliberately a single-node deployment. It does not configure replication, Sentinel, or high availability. The persistent volume is the live data store; backups, restore testing, retention, capacity planning, and disaster recovery remain the operator's responsibility. Treat chart, image, ACL, and storage changes as planned changes.

Valkey retains its default periodic RDB snapshots; this catalog does not enable AOF. Review the snapshot and backup policy for your workload: a PVC preserves files already written to disk, not every acknowledged in-memory write.

Client connections use plain TCP inside the cluster. Use this starter only within a trusted cluster network, or configure the chart's TLS support before exposing it to an untrusted network.

The management `GeneratedSecret` is named `valkey-users-<cluster>` and contains one `password` key. `configurationRef` transports that value to the target Helm render as `configuration.password`. The official chart consumes it through `auth.aclUsers.default.password` and emits the target Secret `valkey-<cluster>-auth` with `default-password`; the ACL is `~* &* +@all`. Do not copy the password into application manifests or source files.

## Local access

Use the target cluster context explicitly and keep the generated password in a hidden shell variable. The following example forwards only to loopback and does not publish Valkey.

In terminal 1:

```bash
export TARGET_CONTEXT="your-target-context"
export VALKEY_RELEASE="valkey-your-cluster"
kubectl --context "$TARGET_CONTEXT" -n valkey port-forward "svc/$VALKEY_RELEASE" 6379:6379
```

In terminal 2, enter the password without echoing it and run a ping. `REDISCLI_AUTH` keeps the password out of the command arguments:

```bash
read -r -s VALKEY_PASSWORD
export REDISCLI_AUTH="$VALKEY_PASSWORD"
redis-cli --user default -h 127.0.0.1 -p 6379 PING
unset REDISCLI_AUTH VALKEY_PASSWORD TARGET_CONTEXT
```

Do not paste the password or command output into logs, tickets, or source files. Stop the port-forward when finished.

## References

- [Valkey Helm chart source](https://github.com/valkey-io/valkey-helm)
- [Valkey security and ACLs](https://valkey.io/topics/acl/)
- [Valkey persistence](https://valkey.io/topics/persistence/)
