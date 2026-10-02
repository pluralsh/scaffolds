# node-pool-resize

Entry point `NodePoolResize`, package `internal/nodepoolresize`. Sets the node count of a
manually scaled GKE node pool. GKE counts nodes per zone, so the pool gets `count` nodes in
each of its zones; a regional pool in three zones with `count: 2` has six nodes. GKE adds
nodes, or drains and removes them, in the background.

Serve it with `just serve node-pool-resize`. Setup, calling conventions and the response format
are in the [README](../README.md).

- [Input](#input)
- [How it behaves](#how-it-behaves)
- [Test fixtures](#test-fixtures)
- [Test matrix](#test-matrix)
- [Cleanup](#cleanup)

## Input

| Field | Required | Notes |
|---|---|---|
| `action` | no, but the tool schema requires it | `plan` or `execute`. Defaults to `plan`. |
| `location` | yes | Region of a regional cluster or zone of a zonal one. The recipes use `$LOCATION`, which defaults to `$ZONE`. |
| `cluster` | yes | Cluster name in `GOOGLE_CLOUD_PROJECT`. |
| `nodePool` | yes | Node pool name. |
| `count` | yes | New node count per zone, 0 to 1000. |

```
just pool-plan    <pool> <count> [cluster]
just pool-execute <pool> <count> [cluster]
```

## How it behaves

Guards, evaluated in this order:

| Guard | Passes when |
|---|---|
| `exists` | The cluster and the pool exist. When either doesn't, this is the only guard in the response. |
| `standard-cluster` | The cluster isn't Autopilot, which manages its nodes itself. |
| `idle` | The cluster and the pool are both `RUNNING`, so no other operation is changing them. |
| `manually-scaled` | The cluster autoscaler is off for the pool. Otherwise change its minimum and maximum instead. |
| `count-allowed` | `count` times the pool's zones is at most `MAX_NODE_COUNT` (terraform `node_pool_max_count`, default 100), and the cluster's only node pool keeps at least one node per zone. |

The current size comes from the target size of the pool's managed instance groups, one per
zone, and is reported as `result.nodePool.nodesPerZone` and `result.from` (the total). If a
group can't be found, its zone is left out and so is `from`. `result.to` is the total once
resized.

```
plan     -> planned, result.from and result.to
execute  -> done, result.submitted true, result.operation set; GKE resizes in the background
execute  -> with every zone already at count: done, submitted false, nothing sent
```

The custom role has `container.clusters.get`, `container.clusters.update` (which setting a
pool's size requires, and which also allows other cluster changes) and
`compute.instanceGroupManagers.get` for the current sizes.

## Test fixtures

```bash
just pool-fixtures      # zonal cluster $P-gke in $ZONE: pools default-pool and apps (manual), auto (autoscaled); ~10 minutes
just pool-show          # pools with status and autoscaling, and their instance groups' target sizes
```

Autopilot clusters, busy pools, regional pools and unknown group sizes are covered by the unit
tests in `internal/nodepoolresize`.

## Test matrix

See [Testing a function](../README.md#testing-a-function) for the conventions. Start the
server with `MAX_NODE_COUNT=3 just serve node-pool-resize` for C4.

### A. Input validation, no GCP calls or credentials needed

| # | Request | Expected |
|---|---|---|
| A1 | `just pool-plan Apps 1` | 400 `nodePool "Apps" is not a GKE node pool name` |
| A2 | `just pool-plan apps -1` | 400 `count -1 is not between 0 and 1000` |
| A3 | `LOCATION=us just pool-plan apps 1` | 400 `location "us" is not a region or zone` |

### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just pool-plan nope 1` | `refused`; only `exists` failed, `cluster or node pool not found` | - |
| B2 | `just pool-plan apps 2` | `planned`; all 5 guards pass; `from` 1, `to` 2, `nodesPerZone` = `{$ZONE: 1}`, `machineType` e2-small | `just pool-show`: apps still 1 |
| B3 | `just pool-plan auto 2` | `refused`; `manually-scaled` failed, `... between 0 and 2 nodes per zone; change those instead` | - |
| B4 | `just pool-plan apps 0` | `planned`; another pool has nodes, so the cluster keeps some | - |

### C. Execute

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just pool-execute auto 2` | `refused`; `manually-scaled` failed; no `result.operation` | auto unchanged |
| C2 | `just pool-execute apps 2` | `done`; `submitted` true, `operation` set | After a few minutes, `just pool-show`: apps' group at 2 |
| C3 | Right after C2: `just pool-execute apps 1` | `refused`; `idle` failed while the pool is `RECONCILING`. Once `RUNNING`: `done` | apps back at 1 |
| C4 | With `MAX_NODE_COUNT=3`: `just pool-execute apps 4` | `refused`; `count-allowed` failed, `... at most 3 in total` | - |
| C5 | `just pool-execute apps 1` when it already has 1 | `done`; `submitted` false, no `operation` | - |

### D. Permissions

Run as the `node-pool-resize` service account, see
[Running as the minimal service account](../README.md#running-as-the-minimal-service-account).

| # | Setup | Expected |
|---|---|---|
| D1 | Repeat B2, B3, C2 and C5 | Same results |
| D2 | `just role-remove node-pool-resize compute.instanceGroupManagers.get`, wait, restart, plan apps | 502 `Provider`, 403 on the instance group |
| D3 | `just role-remove node-pool-resize container.clusters.update`, wait, restart, execute apps 2 | 502 `Provider`, 403 on `setSize`; pool unchanged |

Add the permissions back with `just role-add node-pool-resize <permission>`.

## Cleanup

```bash
just pool-cleanup
just sa-delete node-pool-resize
```
