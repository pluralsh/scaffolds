# Plural cloud functions

Operational cloud functions for work that usually happens outside of IaC, such as cleaning
up resources orphaned by Kubernetes, packaged for each cloud's FaaS and registered as Plural
workbench tools. The function sources live in `functions/` of
[pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).

## Components

- `bootstrap/apps/cloud-functions/<name>/stack.yaml`: InfrastructureStack that deploys the functions.
- `terraform/apps/cloud-functions/<name>`: terraform for the selected cloud.
  - AWS: one arm64 Lambda per function with a least-privilege execution role and a
    CloudWatch log group, an IAM policy allowing only to invoke the functions, and one
    `LAMBDA` workbench tool per function.
  - Azure: one Flex Consumption function app (custom handler) per function, each with its
    own system-assigned managed identity, in the resource group of the mgmt cluster; a role
    allowing only to resolve the function keys, and one `AZURE_FUNCTION` workbench tool per
    function.
  - GCP: one Cloud Run service per function running as its own service account, in the
    project of the mgmt cluster, reachable only by identities granted `roles/run.invoker`,
    and one `CLOUD_RUN` workbench tool per function. The services run the function binary
    on Cloud Run's OS-only base image without a container build, from a private bucket.
    This Cloud Run feature is in Preview.

An init container of the stack run downloads the function packages from the GitHub release
and verifies them against its `SHA256SUMS`, so the mgmt cluster needs to reach `github.com`
and Docker Hub (`curlimages/curl`) while the stack runs. The packages are deployed into
Lambda, Azure and the GCP bucket, so the functions don't depend on the release afterwards.

## Available functions

| Function | Clouds | Changes resources | Description |
|---|---|---|---|
| `volume-delete` | AWS, Azure, GCP | Yes | Deletes an orphaned volume (EBS volume, managed disk, zonal persistent disk) that Kubernetes created for a PersistentVolumeClaim, after snapshotting it. |

All functions are deployed by default; the stack's `functions` variable selects a subset.
Functions that change resources are not registered as workbench tools, and can't be invoked by
the cloud connection, unless `register_destructive_tools` is set. Every call of their tools then
requires human approval in the workbench. On Azure, volume-delete needs the resource group
it may act on (`volumeDeleteScope` when installing, `volume_delete_scopes` in terraform).

### volume-delete

The caller names the volume and the PersistentVolume it was created for (`pvName`). The
function can't see the cluster, so the caller has to confirm that the PersistentVolume no
longer exists first: an unattached volume can still belong to a live PersistentVolume, e.g.
of a StatefulSet scaled to zero. The volume is only deleted when it exists, is not attached,
is in a deletable state and was created by Kubernetes for that PersistentVolume (the
`kubernetes.io/created-for/*` metadata the CSI drivers set). `plan` never changes anything.

A snapshot is always taken first: `execute` starts the snapshot and keeps the volume, and a
later `execute` deletes the volume once the snapshot has completed. A snapshot only counts if
the cloud records it as taken of this volume (not just tagged for it), it is less than 24
hours old and, on GCP and Azure, it was taken after the volume was last detached. AWS doesn't
report detach times, so there only the 24 hours limit it: data written to a volume that is
attached, written and detached again within 24 hours of the snapshot is not in it.
`allow_skip_snapshot` lets callers pass `snapshot: false`; without it the tool has no such
input and the function rejects it.

The function's own permissions are limited as well:

- AWS: deleting and snapshotting are only allowed on volumes carrying the
  `kubernetes.io/created-for/pvc/name` tag, in the function's region. Snapshots of volumes
  encrypted with a customer managed key may also need KMS permissions.
- Azure: the role is only assigned on the resource groups in `volume_delete_scopes`, which
  is required, e.g. the AKS node resource group. It covers every disk there; the Kubernetes
  checks are only done by the function.
- GCP: a project custom role with only the disk and snapshot permissions it needs, for every
  disk in the project; GCP IAM can't restrict it to Kubernetes disks, so those checks are only
  done by the function. Regional disks are not supported.

Only registered functions can be invoked by the cloud connection (the AWS invoke policy, the
GCP `roles/run.invoker` grants and the Azure `invoke_scopes`). The pre-deletion snapshots are
kept; delete them once they are no longer needed.

## After the stack is applied

1. Grant the cloud connection permission to invoke the functions:
   - AWS: attach the `invoke_policy_arn` output to the IAM principal of the cloud connection.
   - Azure: assign the `invoke_role_definition_id` output to the service principal of the
     cloud connection on each of the `invoke_scopes` (the function apps of registered tools).
   - GCP: set the invoker service account when installing, or grant the cloud connection
     service account `roles/run.invoker` on the services.
2. Add the tools from the `workbench_tool_ids` output to a workbench.

## Invocation contract

Every function takes `action`: `plan` evaluates safety checks without changing anything and
`execute` performs the operation if all checks pass. Calls are synchronous, so functions
submit the change and report the resource state without waiting for it to complete.

## Customizations

Terraform variables not set by the stack, such as `log_retention_days` (AWS),
`resource_group_name` and `instance_memory_in_mb` (Azure), `max_instance_count` (GCP) or
`tags`, can be added to `variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
