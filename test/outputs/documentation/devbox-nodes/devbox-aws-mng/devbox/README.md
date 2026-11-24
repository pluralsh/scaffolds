# Devbox node pool

Creates a dedicated node pool for Plural devboxes (Agent Substrate workers) on `devbox-aws-mng` and puts
`/var/lib/ate`, where atelet keeps actor bundles, the gVisor rootfs overlay, durable directories and snapshots, on
fast disk instead of the boot volume.

## What gets created

* An `InfrastructureStack` (`terraform/apps/devbox-nodes/devbox-aws-mng/devbox`) with the node pool:
  * **AWS, Karpenter**: a node role with an access entry. The `NodePool`/`EC2NodeClass` are rendered into the
    pool's service. With `installKarpenter` the stack also creates the controller's IAM, pod identity and
    interruption queue, and the Karpenter chart is installed as its own service; do that once per cluster.
  * **AWS, managed node group**: an EKS managed node group with a launch template. EKS drops the launch template's
    nested virtualization CPU option, so this only supports `gvisor`.
  * **GCP**: a GKE node pool (Ubuntu, raw local NVMe SSDs, nested virtualization for `microvm`).
  * **Azure**: an AKS node pool. Use an L-series size (e.g. `Standard_L16s_v3`) for local NVMe.
* A `devbox-nodes-devbox` service in the `devbox-nodes` namespace with the
  `devbox-disk-prep-devbox` DaemonSet.


Everything is named after the pool, so further pools on `devbox-aws-mng` sit beside this one. Karpenter and
Agent Substrate run once per cluster, so turn off `installKarpenter` when adding a pool to a
cluster that already runs them.

Every node carries the label `plural.sh/devbox-pool=devbox`, the taint
`ate.dev/sandboxClass=gvisor:NoSchedule` (Substrate worker pods tolerate it automatically)
and the startup taint `plural.sh/devbox-disk=pending:NoSchedule`.

## Disk preparation

`devbox-disk-prep` runs on every devbox node. When a node first boots it:

1. finds unpartitioned, unformatted disks whose model matches the cloud's local NVMe (or, on AWS with
   `diskMode: network`, the extra EBS volume),
2. builds an mdadm RAID0 when there are several, formats it XFS (label `ate-data`, mounted with `prjquota` so
   per-actor project quotas are possible) and mounts it at `/var/lib/ate`,
3. removes the `plural.sh/devbox-disk` taint and labels the node `plural.sh/devbox-disk-ready=true`.

atelet doesn't tolerate the startup taint, so it never writes to the boot disk before the data disk is mounted.
Substrate workers do, so the cluster autoscaler can size the pool from its template, and after a reboot the array
is reassembled and remounted with the taint already gone, so atelet can race the remount too. A pod that started
first keeps writing to the boot disk directory the mount hides (a worker's socket atelet can then never reach), so
after each mount `devbox-disk-prep` deletes the node's pods that mount `/var/lib/ate` from the host and started
before it, and their controllers recreate them on the data disk. Disks are only claimed on the node's first boot, so
CSI volumes attached later are never touched.

With GCP/Azure `diskMode: network` there is nothing to format. `/var/lib/ate` stays on the boot disk, so size
`bootDiskSize` for the actors you expect per node.

The DaemonSet runs `/devbox-disk-prep` from the agent-harness base image (`ghcr.io/pluralsh/agent-harness-base:0.6.76`), which
already carries `mdadm`, `xfsprogs` and `util-linux`, so it needs no extra image. Keep it on the deployment
operator's version.

## Point a DevboxRunner at the pool

Run one worker per node by setting requests equal to limits at roughly the node's allocatable resources. The worker's
limits are the capacity Substrate places actors against.

Set `workerPool.diskSize` to the size of the node's `/var/lib/ate` disk: the NVMe capacity of the instance type
with `diskMode: local`, or the data or boot disk size with `diskMode: network`. The operator then sizes devboxes
against the worker. `spec.resources` is each devbox's minimum, and its disk need is `resources.disk` plus
`resources.memory` (for its snapshot), out of 80% of `diskSize` (`diskReservePercent` keeps the rest for the image
cache). The number of devboxes per worker is the most that fit its CPU, memory and disk, and each devbox gets an even
share of the worker's CPU and memory. Substrate places devboxes by those limits, so a worker never takes more devboxes
than its disk can hold. `status.capacity` on the DevboxRunner reports the result.

With `workerPool.autoscaling` the operator sizes the pool to demand. It keeps `spareInstances` free devbox slots, so
a resume doesn't wait for a node. Pending workers make Karpenter or the cluster autoscaler add nodes. To scale down,
it waits `scaleDownDelay` after the last scaling event, drains idle workers in Substrate, and removes only those.
`additionalPools` adds existing WorkerPools the runner may also use; their free slots count toward the spare.

```yaml
apiVersion: deployments.plural.sh/v1alpha1
kind: DevboxRunner
metadata:
  name: platform
spec:
  agents:
  - type: claude
    default: true
  resources: {cpu: "2", memory: 4Gi, disk: 20Gi}
  workerPool:
    autoscaling:
      minReplicas: 1
      maxReplicas: 10
      spareInstances: 2
      scaleDownDelay: 10m
    diskSize: 1700Gi
    # Optional: existing WorkerPools (same sandbox class) the runner's devboxes may also run on.
    # additionalPools:
    # - name: shared-gvisor
    #   namespace: ate-system
    sandboxClass: gvisor
    nodeSelector:
      plural.sh/devbox-pool: devbox
    tolerations:
    # Lets the cluster autoscaler size the pool from its template; atelet still waits for the disk.
    - key: plural.sh/devbox-disk
      operator: Exists
      effect: NoSchedule
    podAnnotations:
      karpenter.sh/do-not-disrupt: "true"
    resources:
      requests: {cpu: "14", memory: 56Gi}
      limits: {cpu: "14", memory: 56Gi}
    # Optional: pin workers to zones, e.g. to keep them next to zonal caches.
    # nodeAffinity:
    #   requiredDuringSchedulingIgnoredDuringExecution:
    #     nodeSelectorTerms:
    #     - matchExpressions:
    #       - {key: topology.kubernetes.io/zone, operator: In, values: [us-east-2a, us-east-2b]}
```

To keep atelet off other
nodes, give it a node affinity for the `plural.sh/devbox-pool` label (operator `Exists`) in the `agent-substrate` chart
values, so it still runs on every devbox pool.

## Disruption

Running actors live on the node until they're snapshotted, so the Karpenter `NodePool` uses
`consolidationPolicy: WhenEmpty`, `expireAfter: Never` and a one-hour `terminationGracePeriod`, so atelet has time to
suspend actors when a node is drained. `karpenter.sh/do-not-disrupt` on the worker pods blocks voluntary
disruption (consolidation, drift) of nodes that run workers. Spot interruptions and the grace period still apply.
Managed node groups, GKE and AKS drain nodes during upgrades. Each pool surges one node at a time.
