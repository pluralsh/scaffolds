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
| `lb-delete` | Deletes an orphaned Application or Network Load Balancer that Kubernetes created for a Service or Ingress, and the target groups it left behind. |
| `db-restore` | Restores an RDS DB instance to a point in time, as a new instance. |

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

Sets the desired size of a manually scaled EKS managed node group or Auto Scaling group.
`node_pool_max_count` (default 100) caps the count.

Takes either `clusterName` + `nodegroupName` (EKS managed node group) or
`autoScalingGroupName` (an Auto Scaling group directly), plus `count`. It refuses:

- groups scaled by the cluster autoscaler: groups tagged `k8s.io/cluster-autoscaler/enabled=true`,
  which for a node group are the Auto Scaling groups behind it. Change the autoscaler's
  minimum and maximum instead;
- Auto Scaling groups that belong to an EKS managed node group (`eks:nodegroup-name` tag): EKS
  owns their size, so resize the node group instead;
- node groups that are not `ACTIVE`, because another update is still running.

EKS and Auto Scaling reject a desired size outside the group's own minimum and maximum, so
`execute` lowers the minimum or raises the maximum when the new size needs it (EKS needs a
maximum of at least 1). `plan` reports the limits it would set as `newMin` and `newMax`.
`execute` returns without waiting for the instances to start or stop.

Permissions: updating is limited to the node groups and Auto Scaling groups of the function's
region and account; describing Auto Scaling groups can't be limited to resources.

### vm-delete

Deletes a standalone EC2 instance together with its root volume and network interfaces. Data
volumes are kept (delete them with `volume-delete` if needed).

Takes `instanceId`. It refuses instances that:

- are in an Auto Scaling group, or are EKS worker nodes (`eks:nodegroup-name` or
  `kubernetes.io/cluster/*` tags): resize their node group with `node-pool-resize` instead, so
  it doesn't replace them;
- have termination protection (`DisableApiTermination`), which the function never turns off;
- have an instance-store root volume;
- are not `running` or `stopped`, for example while starting or stopping.

`execute` first sets delete-on-termination on the root volume and network interfaces and clears
it on the data volumes, then terminates the instance. EC2 applies the change at once, so this
is usually one `execute`; if a read afterwards doesn't show it yet, `execute` is refused and a
later one terminates the instance. Once the result reports `deleteOnTerminationSet`,
terminating the instance in any way also deletes its root volume and network interfaces.

Not covered by the instance's own settings: instance-store volumes are lost with the instance
(`plan` can't list them, since EC2 doesn't report them for an instance), and Elastic IPs stay
allocated and keep being billed, so release them separately.

Permissions: describing instances, their attributes and network interfaces; modifying and
terminating instances, and modifying network interfaces, in the function's region and account.
IAM can't express the Auto Scaling and EKS refusals, so only the function enforces them.

### lb-delete

Deletes a load balancer that the AWS Load Balancer Controller or the Kubernetes cloud provider
created for a Service or Ingress and left behind when it was deleted, and then its target
groups. Only Application and Network Load Balancers are supported: Classic Load Balancers have
no target groups and another API.

The caller names the load balancer (`loadBalancerArn`), the EKS cluster (`clusterName`) and the
Service or Ingress it was created for (`serviceName`, `namespace/name`, or the name of an
Ingress group). The function can't see the cluster, so the caller has to confirm that the
Service or Ingress no longer exists first. The load balancer is only deleted when:

- its tags say it was created for that cluster and Service: `elbv2.k8s.aws/cluster` with
  `service.k8s.aws/stack` or `ingress.k8s.aws/stack` for the controller, or
  `kubernetes.io/cluster/<cluster>=owned` with `kubernetes.io/service-name` for the cloud
  provider;
- deletion protection is off (the function never turns it off) and it is not provisioning;
- none of its target groups has a healthy target or one that is still registering, which would
  mean traffic still reaches it.

Each `execute` makes one change, because a target group can only be deleted once the load
balancer using it is gone. The first deletes the load balancer, with its listeners, and reports
`remaining: true` if it had target groups. Call it again with the same input: the load balancer
is then gone, and the function finds the target groups left for that cluster and Service by
their tags and deletes those that no load balancer uses and that have no healthy or registering
targets. If any of them fails a check, none is deleted. `plan` lists the target groups with their
targets.

Permissions: describing load balancers and target groups can't be limited to resources.
Deleting is limited to load balancers and target groups in the function's region that carry
the tags above, the same condition the AWS Load Balancer Controller's own policy uses. The
function runs with a 60 second timeout, since finding the leftover target groups lists every
target group of the region.

### db-restore

Restores an RDS DB instance (`dbInstanceIdentifier`) to a point in time
(`restorePointInTime`, between the earliest and latest restorable times `plan` reports) as a
new instance (`targetDbInstanceIdentifier`) in the same region and account. The source
instance is never changed: restoring over it or into an existing instance is refused, so
applications only move to the restored data when they are pointed at the new instance. Aurora
DB instances aren't supported (`RestoreDBInstanceToPointInTime` doesn't apply to them). RDS
copies the source's configuration onto the new instance. `execute` submits the restore, which
takes a while; calling again with the same parameters reports the new instance's state and
endpoint. The new instance is tagged with its source and restore point, and is only ever
created: an instance that appears under the target name in the meantime is left alone.

Permissions: describing DB instances and listing tags can't be limited to resources.
Restoring and tagging are limited to DB instances in the function's region and account.
Writing existing instances isn't needed; only the function's checks prevent restoring over
the source.

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
