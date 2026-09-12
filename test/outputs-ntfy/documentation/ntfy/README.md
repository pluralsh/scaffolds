# Plural ntfy

Deploys [ntfy](https://ntfy.sh), an HTTP-based publish/subscribe notification
server, using the community-maintained [HelmForge chart](https://helmforge.dev/docs/charts/ntfy)
listed in the [ntfy installation guide](https://docs.ntfy.sh/install/#helm).

## Deployment

- Chart `1.2.2`, using the official ntfy `v2.28.0` image.
- One replica in namespace `ntfy`, with a `Recreate` update strategy.
- A 2Gi ReadWriteOnce volume at `/var/cache/ntfy` for the SQLite message
  cache and account/access database.
- An internal ClusterIP service named `ntfy` on port 80.
- Anonymous topic access denied, with no preconfigured accounts or passwords.
- Startup, liveness and readiness checks at `/v1/health`.
- No Ingress, Gateway API route, external database, or monitoring CRDs required.

The container runs as UID/GID 1000 with a read-only root filesystem. Its data
volume uses filesystem group 1000, and `/tmp` has a bounded writable volume.
Privilege escalation is disabled, and only `NET_BIND_SERVICE` is added back
after dropping capabilities because this chart listens on port 80.
A dedicated ServiceAccount is created with no Role or RoleBinding.

## Inputs

| Input | Meaning |
| --- | --- |
| `cluster` | Target Kubernetes 1.26 or newer cluster. |
| `baseUrl` | Optional canonical URL for a separately configured ingress. Omit or leave empty for internal access. |
| `storageClass` | Optional class for the 2Gi volume. Omit or leave empty to use the cluster default. |

Setting `baseUrl` does not create an ingress. The value is committed to Git,
so it must not contain credentials. This deployment supports internal HTTP
clients and the web app; iOS instant delivery and browser Web Push need
additional configuration described in the ntfy documentation.

The automation writes the service under `bootstrap/apps/ntfy/<cluster>/`
and Helm values under `helm/ntfy/<cluster>/`. It uses the existing `infra`
GitRepository in namespace `infra`, as other catalog applications do.

## Create the first account

With your Kubernetes context set to the target cluster, wait for the
deployment to become ready and run:

```sh
kubectl -n ntfy rollout status deployment/ntfy
kubectl -n ntfy exec -it deployment/ntfy -- ntfy user add --role=admin admin
```

Enter the password at the interactive prompt. It is stored in the account
database on the persistent volume; do not place passwords or access tokens
in the generated values or Git. Admin users can access all topics. For
application-specific users, follow the upstream [user and ACL guide](https://docs.ntfy.sh/config/#access-control).

In a separate terminal, open an authenticated local connection:

```sh
kubectl -n ntfy port-forward service/ntfy 8080:80
```

Open `http://localhost:8080` and use the account when subscribing to a topic.
You can also publish a test message from another terminal; curl prompts
for the admin password:

```sh
curl -u admin -d "ntfy is ready" http://localhost:8080/example
```

An unauthenticated publish or subscription is rejected. The health endpoint
remains available for the Kubernetes probes.

## Storage and exposure

The StorageClass must provision a writable ReadWriteOnce volume compatible
with filesystem group 1000. Back up the cache/account volume, including the
account database. This is a single-instance SQLite deployment; do not scale
it to multiple writers. Updates briefly interrupt service.

Before exposing the instance, configure TLS and the canonical `baseUrl`, and
create accounts or scoped tokens. Forwarded client-IP headers are not trusted
by default. Configure ntfy's proxy settings explicitly for a trusted ingress
if needed; see the [server configuration guide](https://docs.ntfy.sh/config/).
The chart's configuration checksum restarts the workload when settings change.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
