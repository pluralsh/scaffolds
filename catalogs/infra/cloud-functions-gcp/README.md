# Plural cloud functions for GCP

Operational cloud functions for work that usually happens outside of IaC, such as cleaning
up resources orphaned by Kubernetes, deployed as Cloud Run functions and registered as Plural workbench
tools. The function sources live in `functions/` of
[pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).

## Components

- `bootstrap/apps/cloud-functions/<name>/stack.yaml`: InfrastructureStack that deploys the
  functions, see [Project and credentials](#project-and-credentials).
- `terraform/apps/cloud-functions/<name>`: terraform for GCP: one Cloud Run function (2nd gen)
  per function running as its own service account, in the project of a GKE cluster,
  reachable only by identities granted `roles/run.invoker`, and one `CLOUD_RUN` workbench tool
  per function. The functions are written in Go. Terraform uploads their source to a private
  bucket and Cloud Build builds it as a dedicated service account into an Artifact Registry
  repository of the installation, the only repository that account can write to, so no Preview
  feature is involved.

An init container of the stack run downloads the GCP function package from the GitHub release
and verifies it against its `SHA256SUMS`, so the management cluster needs to reach `github.com` and
Docker Hub (`curlimages/curl`) while the stack runs. The package is the Go source of all
functions (`functions-gcp.zip`), uploaded to a bucket of the installation, so the functions
don't depend on the release afterwards. The project needs the Cloud Functions, Cloud Run,
Cloud Build and Artifact Registry APIs, and the APIs the functions call (Compute Engine, GKE,
Cloud SQL Admin, IAP), which the stack enables.

## Available functions

| Function | Description |
|---|---|
| `volume-delete` | Deletes an orphaned zonal persistent disk that Kubernetes created for a PersistentVolumeClaim, after snapshotting it. |
| `vm-delete` | Deletes a standalone Compute Engine instance with its boot disk, keeping its data disks. |
| `node-pool-resize` | Sets the node count of a manually scaled GKE node pool. |
| `lb-frontend-delete` | Removes what a deleted Kubernetes `LoadBalancer` Service left of its GKE load balancer. |
| `db-restore` | Restores a Cloud SQL instance to a point in time, as a new instance. |
| `ssh-access` | Grants a user or service account short-lived SSH access to an instance through OS Login and IAP. |

Every deployed function is registered as a workbench tool. Every function changes
resources, so every call of their tools requires human approval in the workbench.

The installation deploys all of them. Every function acts in the functions' project,
with a project custom role holding only the permissions it needs; GCP IAM can't narrow most of
them to particular resources, so the functions' checks do that.

All functions share the same safety model: `plan` never changes anything and reports the
checks; `execute` checks again and only acts when every check passes. Functions that need
more time than an invocation make one change per execute and report what is left.

### volume-delete

The caller names the volume and the PersistentVolume it was created for (`pvName`). The
function can't see the cluster, so the caller has to confirm that the PersistentVolume no
longer exists first: an unattached volume can still belong to a live PersistentVolume, e.g.
of a StatefulSet scaled to zero. The volume is only deleted when it exists, is not attached,
is in a deletable state and was created by Kubernetes for that PersistentVolume (the
`kubernetes.io/created-for/*` metadata the CSI drivers set). `plan` never changes anything.

A snapshot is always taken first: `execute` starts the snapshot and keeps the volume, and a
later `execute` deletes the volume once the snapshot has completed. A snapshot only counts if
the cloud records it as taken of this volume (not just tagged for it), is less than 24 hours
old and was taken after the volume was last detached. `allow_skip_snapshot` (the
`allowSkipSnapshot` installation field) lets callers pass `snapshot: false`; without it the
tool has no such input and the function rejects it.

The function's own permissions are limited as well: a project custom role with only the disk
and snapshot permissions it needs, for every disk in the project; GCP IAM can't restrict it to
Kubernetes disks, so those checks are only done by the function. Regional disks are not
supported.

The pre-deletion snapshots are kept; delete them once they are no longer needed.

### vm-delete

Deletes a standalone VM/instance together with its OS disk/volume and network interfaces;
data disks/volumes are kept (delete them with `volume-delete` if needed).

Takes `zone` and `instance`. Instances created by a managed instance group (`created-by`
metadata), GKE nodes (`goog-gke-node` or `goog-k8s-cluster-name` labels), instances with
deletion protection and instances that are starting, stopping or being repaired are refused;
running, stopped and suspended instances can be deleted. Compute Engine network interfaces
aren't separate resources and go with the instance; static external IPs are kept. Local SSDs
are always deleted with the instance, so `plan` lists them (`localSsds`). `execute` first
sets auto-delete on the boot disk and clears it on the data disks, then deletes the instance.
Compute Engine applies the flags in the background, so deleting an instance usually takes two
`execute` calls: the first sets them and is refused, and a later one deletes the instance.
Once the result reports `autoDeleteSet`, deleting the instance in any way also deletes its
boot disk and keeps its data disks. Permissions: a project custom role to read and delete
instances and set their disks' auto-delete flag (`compute.instances.get`, `.delete`,
`.setDiskAutoDelete`, and `compute.disks.update`, which changing the flag requires). It
covers every instance in the project; managed instance group and GKE membership are only
enforced by the function.

### node-pool-resize

Sets the node count of a manually scaled node pool / node group. Autoscaler-owned groups are
refused. `node_pool_max_count` (default 100) caps the count.

Takes `location` (the cluster's region or zone), `cluster`, `nodePool` and `count`. GKE counts
nodes per zone, so the pool gets `count` nodes in each of its zones, and the cap applies to the
total. GKE adds nodes, or drains and removes the ones it takes away, in the background. The
cluster and the pool must be running and idle, Autopilot clusters are refused, and the
cluster's only node pool keeps at least one node per zone. The current size comes from the
pool's managed instance groups. Permissions: `container.clusters.get` and
`container.clusters.update`, which setting a pool's size requires and which also allows other
cluster changes, and `compute.instanceGroupManagers.get`. They cover every cluster in the
project.

### lb-frontend-delete

For a `LoadBalancer` Service, the GKE cloud provider creates a regional forwarding rule named
after the Service UID (`a` and its first 31 hex digits), a target pool (external) or backend
service (internal) of that name, for `externalTrafficPolicy: Local` a health check of that
name, firewall rules `k8s-fw-<name>` and `k8s-<name>-http-hc`, and possibly a static address
of that name, each described as created for the Service (`kubernetes.io/service-name`). When
the cloud provider fails to clean them up, e.g. because the cluster was deleted first, the
caller names the region, the forwarding rule and the deleted Service (`namespace/name`) and
confirms in the cluster that no Service has that UID. Each `execute` then deletes what nothing
uses any more:

1. the forwarding rule;
2. the target pool or backend service, the firewall rules and the address;
3. the health check.

It refuses forwarding rules described as another Service's, rules that don't forward to a target
pool or backend service, and load balancers with a healthy backend (the Service may still exist,
or the nodes still answer the cluster's shared health check). Resources described as another
Service's, targets other forwarding rules use and addresses in use elsewhere are kept; the
cluster's shared node health check and its firewall rule are never touched. Load balancers of
GKE's L4 controller (`k8s2-` names, with GKE subsetting or backend service based external load
balancers) aren't supported. Permissions: read and delete forwarding rules, target pools,
regional backend services, health checks (both kinds), firewall rules and addresses (reading a
target pool's or backend service's backend health needs only its get permission), and list
forwarding rules. They cover every such resource in the project. In a Shared VPC, the firewall
rules are in the host project, where the function can't delete them.

### db-restore

Restores a Cloud SQL instance (`instance`) to a point in time (`restorePointInTime`, between
the earliest and latest restore points `plan` reports) as a new instance (`targetInstance`),
a point-in-time clone. The source is never changed, and restoring over it or into an existing
instance is refused, so applications only move to the restored data when they are pointed at
the new instance. The clone gets the source's settings, such as its tier, network, database
flags and labels. The source must be a running primary with point-in-time recovery (or, for
MySQL, binary logging) enabled. `execute` submits the restore, which takes a while; `plan` with
the same parameters then reports the new instance's state, connection name and addresses.
Cloud SQL can't label a clone, and labelling it afterwards would need permission to change
every instance, so the function can't tell its restore from another instance under that name:
once the target exists it is reported and refused, never changed. Permissions:
`cloudsql.instances.get` and `cloudsql.instances.clone`, for every instance in the project.

### ssh-access

Grants a user or service account (`principal`, an email) SSH login to a Compute Engine
instance (`zone`, `instance`) for `durationMinutes` (default 60, at most
`ssh_access_max_minutes`). Emails in a `.gserviceaccount.com` domain are granted as service
accounts, all others as users. The access goes through OS Login and an Identity-Aware Proxy TCP
tunnel, so no keys are pushed to the instance and no port is opened to the internet. The
instance needs OS Login (`enable-oslogin=TRUE` in its or the project's metadata) and a firewall
rule letting IAP (`35.235.240.0/20`) reach port 22.

`execute` grants `roles/compute.osLogin` (or, with `role: admin`, `roles/compute.osAdminLogin`)
on the instance and `roles/iap.tunnelResourceAccessor` on its tunnel, and returns the command
to connect: `gcloud compute ssh <instance> --tunnel-through-iap`. Both bindings carry an IAM
condition that ends them at the expiry, so the access ends without any cleanup job. Every
`execute` also removes the function's expired bindings on the instance. Granting again extends
the access and never shortens it, and `revoke: true` removes it right away. SSH sessions opened
before the expiry aren't closed. The function doesn't create keys, change the instance or its
metadata, or open firewall rules.

If the instance runs as a service account, OS Login also requires `roles/iam.serviceAccountUser`
on it, which the function doesn't grant. The result names the account. Access the principal has
through other bindings on the instance is listed as `otherAccess` and is neither granted nor
revoked. Only bindings whose condition has the function's title are changed. Anyone who can set
the policy could create such a binding, so treat the title as a label, not a proof.
Permissions: read instances and project metadata, and read and set the IAM policies of
instances and IAP tunnel instances. Setting a policy allows granting any role on that resource,
and only the function restricts it to these roles.

## Project and credentials

The stack runs on the management cluster, as its `stacks` service account. The functions go
to the project of the GKE cluster `cluster` (the `cluster` variable), read from the
`plrl/clusters/<cluster>` service context the GCP bootstrap creates for each cluster:
`plrl/clusters/mgmt` for a GKE management cluster (the default), `plrl/clusters/<handle>` for
a GKE workload cluster, e.g. when the management cluster runs on another cloud.

Terraform authenticates to GCP as one of:

- the GKE workload identity of the `stacks` service account, on a GKE management cluster from
  the GCP bootstrap. Nothing needs to be set up.
- workload identity federation, when the management cluster runs on another cloud and stacks
  already reach GCP that way, e.g. the stack of a GKE workload cluster: reuse its secret with
  the federation config (`google-application-credentials.json`, whose credential source is the
  file `/var/run/secrets/tokens/gcp-identity-token`), e.g. `gcp-wif-credentials`. Set
  `credentialsSecret` to its name and `workloadIdentityAudience` to the provider's audience.
  The stack mounts the config, sets `GOOGLE_APPLICATION_CREDENTIALS` and projects a `stacks`
  token for that audience at that path.
- a service account key, stored the same way as `google-application-credentials.json` in a
  secret named by `credentialsSecret`, without `workloadIdentityAudience`.

The controller reads `credentialsSecret` from the namespace of the stack object, `apps` on the
management cluster, and stacks can't reference secrets in other namespaces, so a secret kept
elsewhere has to be copied there (the stack's runs, like every stack's, run in
`plrl-deploy-operator`):

```bash
# copy a workload cluster stack's federation config from infra to apps
kubectl -n infra get secret gcp-wif-credentials -o json \
  | jq '{apiVersion, kind, type, data, metadata: {name: .metadata.name, namespace: "apps"}}' | kubectl apply -f -
# or store a service account key
kubectl -n apps create secret generic gcp-functions-creds --from-file=google-application-credentials.json=key.json
```

Either identity needs to manage what the stack creates in the project: the APIs, the
functions' and build service accounts with their custom roles and project bindings, the
Artifact Registry repository, the source bucket, the Cloud Run functions and their invoker
bindings, e.g. `roles/serviceusage.serviceUsageAdmin`, `roles/iam.serviceAccountAdmin`,
`roles/iam.serviceAccountUser`, `roles/iam.roleAdmin`, `roles/resourcemanager.projectIamAdmin`,
`roles/artifactregistry.admin`, `roles/storage.admin`, `roles/cloudfunctions.admin` and
`roles/run.admin`. Managing project IAM bindings and custom roles makes it close to a project
owner, and federated identities need `roles/iam.workloadIdentityUser` on it, or the roles
granted to the federated principal. The stack's job spec replaces the one in the deployment
settings (it adds an init container), so credentials configured only there don't reach this
stack; use `credentialsSecret`.

## After the stack is applied

1. If the installation didn't set `invokerServiceAccount` (the cloud connection's service
   account), grant that account `roles/run.invoker` on the Cloud Run services behind the
   functions.
2. Add the tools from the `workbench_tool_ids` output to a workbench.

## Invocation contract

Every function takes `action`: `plan` evaluates safety checks without changing anything and
`execute` performs the operation if all checks pass. Calls are synchronous, so functions
submit the change and report the resource state without waiting for it to complete.

## Customizations

The installation can also set the functions' limits: `allowSkipSnapshot`
(`allow_skip_snapshot`, off by default), `nodePoolMaxCount` (`node_pool_max_count`, default
100) and `sshAccessMaxMinutes` (`ssh_access_max_minutes`, default 240).

Other terraform variables, such as `functions`, `max_instance_count`, `release_retention_days`
(how long the source and images of earlier releases are kept) or `labels`, can be added to
`variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
