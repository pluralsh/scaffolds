# Plural cloud functions for GCP

Operational cloud functions for work that usually happens outside of IaC, such as cleaning
up resources orphaned by Kubernetes, deployed as Cloud Run functions and registered as Plural workbench
tools. The function sources live in `functions/` of
[pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).

## Components

- `bootstrap/apps/cloud-functions/<name>/stack.yaml`: InfrastructureStack that deploys the functions.
- `terraform/apps/cloud-functions/<name>`: terraform for GCP: one Cloud Run function (2nd gen)
  per function running as its own service account, in the project of the mgmt cluster,
  reachable only by identities granted `roles/run.invoker`, and one `CLOUD_RUN` workbench tool
  per function. The functions are written in Go. Terraform uploads their source to a private
  bucket and Cloud Build builds it as a dedicated service account into an Artifact Registry
  repository of the installation, the only repository that account can write to, so no Preview
  feature is involved.

An init container of the stack run downloads the GCP function package from the GitHub release
and verifies it against its `SHA256SUMS`, so the mgmt cluster needs to reach `github.com` and
Docker Hub (`curlimages/curl`) while the stack runs. The package is the Go source of all
functions (`functions-gcp.zip`), uploaded to a bucket of the installation, so the functions
don't depend on the release afterwards. The project needs the Cloud Functions, Cloud Run,
Cloud Build and Artifact Registry APIs, which the stack enables.

## Available functions

| Function | Description |
|---|---|
| `volume-delete` | Deletes an orphaned zonal persistent disk that Kubernetes created for a PersistentVolumeClaim, after snapshotting it. |
| `vm-delete` | Deletes a standalone Compute Engine instance with its boot disk, keeping its data disks. |

Every deployed function is registered as a workbench tool. Every function changes
resources, so every call of their tools requires human approval in the workbench.

The installation deploys all of them.

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

## After the stack is applied

1. Set the invoker service account when installing, or grant the cloud connection service
   account `roles/run.invoker` on the Cloud Run services behind the functions.
2. Add the tools from the `workbench_tool_ids` output to a workbench.

## Invocation contract

Every function takes `action`: `plan` evaluates safety checks without changing anything and
`execute` performs the operation if all checks pass. Calls are synchronous, so functions
submit the change and report the resource state without waiting for it to complete.

## Customizations

The installation can also set `allowSkipSnapshot` (`allow_skip_snapshot`, off by default).

Other terraform variables, such as `functions`, `max_instance_count`, `release_retention_days`
(how long the source and images of earlier releases are kept) or `labels`, can be added to
`variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
