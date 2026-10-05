# Plural cloud functions for Azure

Operational cloud functions for work that usually happens outside of IaC, such as cleaning
up resources orphaned by Kubernetes, deployed as Azure Functions and registered as Plural workbench
tools. The function sources live in `functions/` of
[pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).

## Components

- `bootstrap/apps/cloud-functions/<name>/stack.yaml`: InfrastructureStack that deploys the functions.
- `terraform/apps/cloud-functions/<name>`: terraform for Azure: one Flex Consumption function
  app (custom handler) per function, each with its own system-assigned managed identity and its
  own storage account for its package and keys, in the resource group of the mgmt cluster,
  sending its logs and invocations to an Application Insights resource backed by a Log
  Analytics workspace (`application_insights_id` output); a role allowing only to resolve each
  function's URL and its own key (not the app's host or master keys), and one `AZURE_FUNCTION`
  workbench tool per function. The functions' permissions are custom roles defined in the
  subscription and assigned on the resource groups they act on (and on the subscription for
  reads Azure only serves there), so the stack's identity needs to define and assign roles in
  the subscription (see below).

The stack runs as the bootstrap's stack identity, `<cluster>-plrl-stacks`, through the service account in `plrl-deploy-operator` its federated credential trusts: `stacks` by default, or `stacksServiceAccount`. The stack pod mounts that service account's token for Azure itself, so it doesn't need the Azure workload identity webhook and works on mgmt clusters outside AKS too. Besides Owner of the
mgmt cluster's resource group, it needs User Access Administrator on the subscription, which
newer bootstraps grant. For an installation bootstrapped before that, someone with Owner on the
subscription grants it once:

```bash
az role assignment create --role "User Access Administrator" --assignee-principal-type ServicePrincipal --assignee-object-id "$(az identity show -g <mgmt resource group> -n <cluster>-plrl-stacks --query principalId -o tsv)" --scope "/subscriptions/<subscription ID>"
```

An init container of the stack run downloads the Azure function packages from the GitHub
release and verifies them against its `SHA256SUMS`, so the mgmt cluster needs to reach
`github.com` and Docker Hub (`curlimages/curl`) while the stack runs. The packages are deployed
into the function apps, so the functions don't depend on the release afterwards.

## Available functions

| Function | Description |
|---|---|
| `volume-delete` | Deletes an orphaned managed disk that Kubernetes created for a PersistentVolumeClaim, after snapshotting it. |
| `node-pool-resize` | Sets the node count of a manually scaled AKS node pool. |
| `vm-delete` | Deletes a standalone VM with its OS disk and network interfaces, keeping its data disks. |
| `lb-frontend-delete` | Removes what a deleted Kubernetes `LoadBalancer` Service left on an AKS load balancer. |
| `db-restore` | Restores a PostgreSQL or MySQL flexible server to a point in time, as a new server. | | `ssh-
access` | Grants an Entra ID user short-lived SSH login to a Linux VM. |

Every deployed function is registered as a workbench tool. Every function changes
resources, so every call of their tools requires human approval in the workbench.

The installation deploys all of them for the AKS clusters in one resource group
(`clusterResourceGroup`). Every function gets its permissions only on the resource groups it may
act on, which terraform derives from that group and its clusters (`cluster_resource_group`;
`scopes` in terraform overrides them per function key). All fields take resource group names:

| Resource group | Functions |
|---|---|
| the clusters' node resource groups (`MC_...`) | `volume-delete`, `lb-frontend-delete` |
| `clusterResourceGroup` | `node-pool-resize` |
| `vmResourceGroup`, by default `clusterResourceGroup` | `vm-delete`, `ssh-access` |
| `databaseResourceGroup`, by default `clusterResourceGroup` (where bootstrap puts Console's database) | `db-restore` |
| the node pools' networks (the node resource group for an AKS-managed VNet), and `networkResourceGroup` (optional) for others such as a hub VNet, public IP prefixes or private DNS zones | join actions only, for `node-pool-resize`, `lb-frontend-delete` and `db-restore` (`network_scopes` in terraform) |

`sshBastionId` (optional) is the Bastion host `ssh-access` users connect through, and
`functionsResourceGroup` (optional) the resource group for the function apps, by default the mgmt
cluster's; set it when the mgmt cluster isn't on AKS.

The stack's `scopes` variable can be edited later, e.g. to give a function more resource
groups.

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

The function's own permissions are limited as well: the role is only assigned on the resource
groups in `scopes["volume-delete"]`, e.g. the AKS node resource group. It covers every disk
there; the Kubernetes checks are only done by the function.

The pre-deletion snapshots are kept; delete them once they are no longer needed.

### node-pool-resize

Sets the node count of a manually scaled node pool / node group. Autoscaler-owned groups are
refused. `node_pool_max_count` (default 100) caps the count.

Takes `clusterId`, `nodePool`, `count`. AKS adds nodes, or cordons and drains the ones it
removes, in the background. The pool must be idle and running. System pools keep at least one
node. The update is sent with the pool's ETag, so a pool changed in the meantime isn't
overwritten. Only scale set pools are supported, not `VirtualMachines` pools. Permissions:
read and write agent pools in `scopes["node-pool-resize"]`, the resource groups of the
clusters. The update resends the whole pool, so Azure also checks that the function may join
the subnets and the public IP prefix the pool references: it gets those join actions there
and in `network_scopes`, which needs the resource group of a cluster's own VNet when it is
another one.

### vm-delete

Deletes a standalone VM/instance together with its OS disk/volume and network interfaces;
data disks/volumes are kept (delete them with `volume-delete` if needed).

Takes `vmId`. Public IPs follow the delete option of their network interface. VMs of scale
sets, AKS nodes (`aks-managed-*` tags) and VMs with unmanaged OS disks are refused.
`execute` first sets the delete options on the VM and then deletes it. Azure usually reports
the VM as updating for a while after the first step, so deleting a VM usually takes two
`execute` calls: the first sets the delete options and is refused while the VM updates, and
a later one, once it has finished, deletes it. Both are sent with the VM's ETag, so a VM changed in
the meantime is left alone. Once the result reports `deleteOptionsSet`, deleting the VM in
any way also deletes its OS disk and network interfaces. Permissions in
`scopes["vm-delete"]`: read, update and delete VMs, and read, update and delete disks and
network interfaces. They cover every VM there, so scope it to the resource groups of such VMs.
It deletes the network interfaces and OS disk, and updates the VM's references to its data
disks, so a disk or network interface in another resource group needs that resource group in
`scopes["vm-delete"]` too; `network_scopes` isn't enough.

### lb-frontend-delete

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
cleaned up by hand. Public IPs AKS didn't create for the frontend are kept. Permissions in
`scopes["lb-frontend-delete"]`, the node resource group: read, update and delete load
balancers, read and delete public IPs, read backend health, and join public IPs, public IP
prefixes, subnets and virtual networks, which updating a load balancer requires for its
remaining frontends and IP-based backend pools. It gets the same join actions in
`network_scopes`, which needs the resource group of a custom virtual network, outbound
public IPs or public IP prefixes the load balancer references; otherwise Azure refuses the
update. The backend health check is a long-running action whose result Azure serves at
subscription scope, so the function can also read Network operation results in the
subscription.

### db-restore

Restores a PostgreSQL or MySQL flexible server (`serverId`) to a point in time
(`restorePointInTime`, between the earliest restore point `plan` reports and now) as a new
server (`targetServerName`) in the same resource group. The source server is never changed:
restoring over it or into an existing server is refused, so applications only move to the
restored data when they are pointed at the new server. The new server gets the source's
network settings (subnet and private DNS zone, or public access), availability zone, admin
login, and, for a server encrypted with a customer managed key, its key and the
user-assigned identities that read it; Azure doesn't copy firewall rules or private
endpoints. `execute` submits the restore, which takes a while, and reports the new server as `Provisioning`; check its progress with a read-only query of the new server. A restore is submitted once: Azure drops the tags the function sends to mark it, so calling again finds an existing server it can't tell from any other and refuses. The new server is only ever created: a server that appears under the target name in the meantime is left alone. Permissions in
`scopes["db-restore"]`: read and write flexible servers, and join subnets and private DNS
zones for servers in a virtual network, which it can also join in `network_scopes`, e.g. a
private DNS zone in a hub resource group, and assign user-assigned identities, so identities
in another resource group need it in the scopes too. Writing servers also allows changing
existing ones; only the function's checks prevent that.

### ssh-access

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
Its permission to manage role assignments on `scopes["ssh-access"]` (the VM resource group) carries a role
assignment condition that only allows the two VM login roles and users, so it can't grant
anything else.

## After the stack is applied

1. Unless the installation set `invokerPrincipalId` (the object ID of the cloud connection's
   service principal, `invoker_principal_id` in terraform), which assigns it already: assign the
   `invoke_role_definition_id` output to the cloud connection's service principal on each of the
   `invoke_scopes` (the function apps), so it can invoke the functions.
2. Add the tools from the `workbench_tool_ids` output to a workbench.

## Invocation contract

Every function takes `action`: `plan` evaluates safety checks without changing anything and
`execute` performs the operation if all checks pass. Calls are synchronous, so functions
submit the change and report the resource state without waiting for it to complete.

## Customizations

The installation can also set the functions' limits: `allowSkipSnapshot`
(`allow_skip_snapshot`, off by default), `nodePoolMaxCount` (`node_pool_max_count`, default
100) and `sshAccessMaxMinutes` (`ssh_access_max_minutes`, default 240).

Other terraform variables, such as `functions`, `scopes`, `network_scopes`,
`resource_group_name`, `instance_memory_in_mb`, `maximum_instance_count`, `log_retention_days`
or `tags`, can be added to or changed in `variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
