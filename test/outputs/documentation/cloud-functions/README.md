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
    allowing only to resolve each function's URL and its own key (not the app's host or
    master keys), and one `AZURE_FUNCTION` workbench tool per function.
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

| Function | Clouds | Description |
|---|---|---|
| `volume-delete` | AWS, Azure, GCP | Deletes an orphaned volume (EBS volume, managed disk, zonal persistent disk) that Kubernetes created for a PersistentVolumeClaim, after snapshotting it. |
| `node-pool-resize` | Azure | Sets the node count of a manually scaled AKS node pool. |
| `vm-delete` | Azure | Deletes a standalone VM with its OS disk and network interfaces, keeping its data disks. |
| `lb-frontend-delete` | Azure | Removes what a deleted Kubernetes `LoadBalancer` Service left on an AKS load balancer. |
| `db-restore` | Azure | Restores a PostgreSQL or MySQL flexible server to a point in time, as a new server. |
| `ssh-access` | Azure | Grants an Entra ID user short-lived SSH login to a Linux VM. |

Only `volume-delete` is deployed by default; the stack's `functions` variable selects others.
Every function changes resources, so none is registered as a workbench tool, or can be
invoked by the cloud connection, unless `register_destructive_tools` is set. Every call of
their tools then requires human approval in the workbench.

On Azure, every function also needs the resource groups it may act on in `scopes`, by
function key, and gets its permissions only there. The installation sets
`scopes["volume-delete"]` from `volumeDeleteScope`; to deploy more functions, add them to
`functions` and to that `scopes` map in the stack's variables, e.g.:

```yaml
functions: [volume-delete, lb-frontend-delete, node-pool-resize]
scopes:
  volume-delete: [/subscriptions/<id>/resourceGroups/MC_<group>_<cluster>_<location>]
  lb-frontend-delete: [/subscriptions/<id>/resourceGroups/MC_<group>_<cluster>_<location>]
  node-pool-resize: [/subscriptions/<id>/resourceGroups/<cluster resource group>]
```

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
- Azure: the role is only assigned on the resource groups in `scopes["volume-delete"]`, e.g.
  the AKS node resource group. It covers every disk there; the Kubernetes checks are only
  done by the function.
- GCP: a project custom role with only the disk and snapshot permissions it needs, for every
  disk in the project; GCP IAM can't restrict it to Kubernetes disks, so those checks are only
  done by the function. Regional disks are not supported.

The pre-deletion snapshots are kept; delete them once they are no longer needed.

### node-pool-resize (Azure)

Sets the node count of an AKS node pool (`clusterId`, `nodePool`, `count`). AKS adds nodes,
or cordons and drains the ones it removes, in the background. The pool must be idle and
running, and pools scaled by the cluster autoscaler are refused, since the autoscaler owns
their count. System pools keep at least one node, and `node_pool_max_count` (default 100)
caps the count. The update is sent with the pool's ETag, so a pool changed in the meantime
isn't overwritten. Only scale set pools are supported, not `VirtualMachines` pools.
Permissions: read and write agent pools in `scopes["node-pool-resize"]`, the resource groups
of the clusters.

### vm-delete (Azure)

Deletes a standalone VM (`vmId`) together with its OS disk and network interfaces; data disks
are detached and kept (delete them with `volume-delete` if needed), and public IPs follow the
delete option of their network interface. VMs of scale sets, AKS nodes (`aks-managed-*` tags)
and VMs with unmanaged OS disks are refused. `execute` first sets the delete options on the
VM and then deletes it; if the VM is still updating, a later `execute` deletes it. Both are
sent with the VM's ETag, so a VM changed in the meantime is left alone. Once the result
reports `deleteOptionsSet`, deleting the VM in any way also deletes its OS disk and network
interfaces. Permissions
in `scopes["vm-delete"]`: read, update and delete VMs, and read, update and delete disks and
network interfaces. They cover every VM there, so scope it to the resource groups of such VMs.

### lb-frontend-delete (Azure)

AKS puts every `LoadBalancer` Service on a shared load balancer (`kubernetes` or
`kubernetes-internal` in the node resource group) as a frontend named after the Service UID
(`a` and its first 31 hex digits), with load balancing rules and health probes, and a public
IP tagged `k8s-azure-service` for public Services. When the cloud provider fails to clean
them up, the caller names the load balancer, the frontend and the deleted Service
(`namespace/name`) and confirms in the cluster that no Service has that UID. Each `execute`
then makes one change:

1. removes the frontend, its rules and the probes only those rules use, sent with the load
   balancer's ETag, or deletes the load balancer if it was its last frontend and it has no
   backends;
2. deletes the public IP once it is no longer in use, if AKS created it for this Service
   alone.

Every change is sent with the ETag the checks were made on. It refuses frontends whose rules
have healthy backends or whose backend health isn't reported in time (the Service may still
exist), frontends shared with other Services (rules named after another Service UID), and
frontends used by outbound or NAT rules. It also refuses the last frontend of a load
balancer that still has backends: removing it would need taking the nodes out of the
backend pools first, which is the cloud provider's job, so such a load balancer has to be
cleaned up by hand. Public IPs AKS didn't create for the frontend are kept. Permissions in `scopes["lb-frontend-delete"]`, the node resource group: read, update
and delete load balancers, read and delete public IPs, read backend health, and join public
IPs and subnets, which updating a load balancer requires for its remaining frontends.
Resources the load balancer references in other resource groups, such as a custom virtual
network, outbound public IPs or public IP prefixes, need those resource groups in the scopes
too; otherwise Azure refuses the update. The same applies to vm-delete (disks and network
interfaces in another resource group) and db-restore (a private DNS zone in a shared
resource group).

### db-restore (Azure)

Restores a PostgreSQL or MySQL flexible server (`serverId`) to a point in time
(`restorePointInTime`, between the earliest restore point `plan` reports and now) as a new
server (`targetServerName`) in the same resource group. The source server is never changed:
restoring over it or into an existing server is refused, so applications only move to the
restored data when they are pointed at the new server. The new server gets the source's
network settings (subnet and private DNS zone, or public access) and admin login; Azure
doesn't copy firewall rules or private endpoints. `execute` submits the restore, which takes
a while; calling again with the same parameters reports the new server's state and hostname.
The new server is tagged with its source and restore point, and is only ever created: a
server that appears under the target name in the meantime is left alone. Permissions in
`scopes["db-restore"]`: read and write flexible servers, and join subnets and private DNS
zones for servers in a virtual network. Writing servers also allows changing existing ones;
only the function's checks prevent that. Servers encrypted with customer managed keys aren't
supported.

### ssh-access (Azure)

Grants an Entra ID user (`principalId`) SSH login to a Linux VM (`vmId`) for
`durationMinutes` (default 60, at most `ssh_access_max_minutes`), using Microsoft Entra ID
login for Linux: no keys are pushed to the VM and no port is opened. The VM needs the
`AADSSHLoginForLinux` extension. `execute` assigns the Virtual Machine User Login role (or,
with `role: admin`, Virtual Machine Administrator Login) on the VM and returns the command to
connect: `az network bastion ssh` through `ssh_bastion_id` when set (Bastion Standard SKU or
higher with native client support), `az ssh vm` otherwise. Granting again extends the access
and never shortens it, and `revoke: true` removes it right away. Access the user has through
other VM login role assignments, at the VM or above it, is listed as `otherAccess` and is
neither granted nor revoked by the function.

Azure role assignments don't expire, so the expiry is recorded in the assignment's
description, and a timer in the function app removes expired assignments every 5 minutes;
so does every `execute` on the VM. Access can therefore last up to 5 minutes longer, and SSH
sessions opened before the expiry aren't closed. Only VM login assignments of users whose
description carries the function's marker are removed; anyone who can write role
assignments could create such an assignment, so treat the marker as a label, not a proof.
Its permission to manage role assignments on `scopes["ssh-access"]` carries a role
assignment condition that only allows the two VM login roles and users, so it can't grant
anything else.

Only registered functions can be invoked by the cloud connection (the AWS invoke policy, the
GCP `roles/run.invoker` grants and the Azure `invoke_scopes`).

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

Terraform variables not set by the stack, such as `functions`, `log_retention_days` (AWS),
`scopes`, `node_pool_max_count`, `ssh_access_max_minutes`, `ssh_bastion_id`,
`resource_group_name` and `instance_memory_in_mb` (Azure), `max_instance_count` (GCP) or
`tags`, can be added to `variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
