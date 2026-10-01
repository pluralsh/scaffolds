# Plural cloud functions for AWS

Operational cloud functions for work that usually happens outside of IaC, such as cleaning
up resources orphaned by Kubernetes, deployed as AWS Lambda and registered as Plural workbench
tools. The function sources live in `functions/` of
[pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).

## Components

- `bootstrap/apps/cloud-functions/<name>/stack.yaml`: InfrastructureStack that deploys the functions.
- `terraform/apps/cloud-functions/<name>`: terraform for AWS: one arm64 Lambda per function
  with a least-privilege execution role and a CloudWatch log group, an IAM policy allowing only
  to invoke the functions, and one `LAMBDA` workbench tool per function.

An init container of the stack run downloads the AWS function packages from the GitHub
release and verifies them against its `SHA256SUMS`, so the mgmt cluster needs to reach
`github.com` and Docker Hub (`curlimages/curl`) while the stack runs. The packages are deployed
into Lambda, so the functions don't depend on the release afterwards.

## Available functions

| Function | Description |
|---|---|
| `volume-delete` | Deletes an orphaned EBS volume that Kubernetes created for a PersistentVolumeClaim, after snapshotting it. |
| `node-pool-resize` | Sets the desired size of a manually scaled EKS managed node group or Auto Scaling group. |
| `vm-delete` | Deletes a standalone EC2 instance with its root volume and network interfaces, keeping its data volumes. |

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
the cloud records it as taken of this volume (not just tagged for it) and is less than 24 hours
old. AWS doesn't report detach times, so only the 24 hours limit it: data written to a volume
that is attached, written and detached again within 24 hours of the snapshot is not in it.
`allow_skip_snapshot` (the `allowSkipSnapshot` installation field) lets callers pass `snapshot:
false`; without it the tool has no such input and the function rejects it.

The function's own permissions are limited as well: deleting and snapshotting are only allowed
on volumes carrying the `kubernetes.io/created-for/pvc/name` tag, in the function's region.
Snapshots of volumes encrypted with a customer managed key may also need KMS permissions.

The pre-deletion snapshots are kept; delete them once they are no longer needed.

### node-pool-resize

Sets the node count of a manually scaled node pool / node group. Autoscaler-owned groups are
refused. `node_pool_max_count` (default 100) caps the count.

Takes either `clusterName` + `nodegroupName` (EKS managed node group) or
`autoScalingGroupName` (ASG directly), plus `count`. Groups tagged
`k8s.io/cluster-autoscaler/enabled=true` are refused. EKS node groups must be `ACTIVE`.
`execute` submits the new desired size (raising `max` when needed) and returns without waiting
for instances. Permissions: describe and update EKS node groups and Auto Scaling groups in the
region.

### vm-delete

Deletes a standalone VM/instance together with its OS disk/volume and network interfaces;
data disks/volumes are kept (delete them with `volume-delete` if needed).

Takes `instanceId`. Instances in an Auto Scaling group, EKS worker nodes
(`eks:nodegroup-name` or `kubernetes.io/cluster/*` tags) and instances with an instance-store
root volume are refused. `execute` first sets delete-on-termination on the root volume and
network interfaces (and clears it on data volumes), then terminates the instance; if the
instance is still updating, a later `execute` terminates it. Once the result reports
`deleteOnTerminationSet`, terminating the instance also deletes its root volume and network
interfaces. Permissions: describe instances and network interfaces in the region, and modify
or terminate those resources. EKS and Auto Scaling membership are only enforced by the
function.

## After the stack is applied

1. Attach the `invoke_policy_arn` output to the IAM principal of the cloud connection, so it
   can invoke the functions.
2. Add the tools from the `workbench_tool_ids` output to a workbench.

## Invocation contract

Every function takes `action`: `plan` evaluates safety checks without changing anything and
`execute` performs the operation if all checks pass. Calls are synchronous, so functions
submit the change and report the resource state without waiting for it to complete.

## Customizations

The installation can also set the functions' limits: `allowSkipSnapshot`
(`allow_skip_snapshot`, off by default) and `nodePoolMaxCount` (`node_pool_max_count`, default
100).

Other terraform variables, such as `functions`, `log_retention_days` or `tags`, can be added to
`variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
