# Plural Memos

Deploys [Memos](https://usememos.com), a Markdown notes and knowledge base
application, for internal team notes and runbooks.

## Deployment

- Official `neosmemo/memos:0.30.0` image pinned to its multi-platform digest.
- One replica, `Recreate` updates and a 2Gi ReadWriteOnce volume for the
  SQLite database and local attachments under `/var/opt/memos`.
- An internal ClusterIP service named `memos`, listening on port 5230.
- Anonymous instance access disabled by leaving `MEMOS_INSTANCE_URL` empty,
  as defined by Memos 0.30.0. Notes default to private.
- Public registration disabled by a deployment-managed GENERAL policy;
  the first user can still bootstrap the admin account on an empty database.
- A dedicated ServiceAccount with token mounting disabled and no RBAC grants.
- UID/GID and filesystem group 10001, matching the official image's non-root
  user. No privilege escalation, dropped capabilities, a read-only root
  filesystem, and a bounded writable `/tmp`.
- No Helm chart, ingress controller, external database or additional CRDs.

The pod disables service-link environment variables and explicitly sets
`MEMOS_PORT=5230`. This prevents a Service named `memos` from injecting a
`MEMOS_PORT=tcp://...` value that Memos would interpret as a port number.

## Inputs and generated files

`cluster` selects a Kubernetes 1.26 or newer cluster. The optional
`storageClass` selects the 2Gi data volume's class; omission or an empty
string uses the cluster's default StorageClass.

The automation writes the service under `bootstrap/apps/memos/<cluster>/`
and standard Kubernetes manifests under `kubernetes/memos/<cluster>/`.
It uses the existing `infra` GitRepository in namespace `infra`.

## Bootstrap and verify

Keep access limited to the operator until the first account is created.
ClusterIP limits external exposure but is not a network access-control policy.
With the Kubernetes context set to the selected target cluster:

```sh
kubectl -n memos rollout status deployment/memos
kubectl -n memos port-forward service/memos 5230:5230
```

Open `http://localhost:5230` and create the first admin. Use a new password;
the catalog contains no account, password or token. The admin can then create
additional users. Subsequent anonymous sign-ups are disabled by the policy.

The health endpoint is deliberately available without authentication:

```sh
curl --fail http://localhost:5230/healthz
```

It returns `Service ready.` after startup. This is an HTTP process check,
not a continuous SQLite health check. Create a private note, restart the
deployment and verify the note is still present after signing in again.
An unauthenticated request to `/api/v1/memos` should return HTTP 401.

## Policy and external access

`memos-policy` supplies `memos-instance-setting-general.json` under
`/etc/secrets`. In the pinned version, this overrides the GENERAL settings
at runtime; edit those settings in Git instead of expecting a UI edit to
override them. Restart the deployment after changing the ConfigMap because
Memos loads deployment configuration at startup:

```sh
kubectl -n memos rollout restart deployment/memos
```

Before enabling external access, finish admin bootstrap and configure a
trusted reverse proxy with TLS. Keep `MEMOS_INSTANCE_URL` empty for the
private mode used here; in version 0.30.0, setting it enables anonymous
instance access. Revisit this behavior when upgrading, since later releases
may separate canonical URL and access settings. Do not commit credentials
or expose the initial admin-creation screen to an untrusted network.

## Storage and upgrades

The StorageClass must provision a writable volume compatible with filesystem
group 10001. Back up the database and all local attachments together, using
a SQLite-consistent backup or a snapshot while the application is stopped.
Do not scale this SQLite deployment to multiple replicas. Updates cause a
brief interruption and may run schema migrations; preserve a pre-upgrade
backup and follow upstream release notes before changing the image digest.

## Sources

- [Memos Kubernetes guidance](https://usememos.com/docs/deploy/kubernetes)
- [Pinned image and entrypoint](https://github.com/usememos/memos/blob/2036c1ffc1b0a1e1fa6a473738c2a5ef520df67f/scripts/Dockerfile)
- [Pinned private-mode behavior](https://github.com/usememos/memos/blob/2036c1ffc1b0a1e1fa6a473738c2a5ef520df67f/internal/profile/profile.go)
- [Deployment configuration loader](https://github.com/usememos/memos/blob/2036c1ffc1b0a1e1fa6a473738c2a5ef520df67f/store/deployment_config.go)

Contributions are welcome at https://github.com/pluralsh/scaffolds.
