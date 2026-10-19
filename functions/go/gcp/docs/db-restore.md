# db-restore

Entry point `DBRestore`, package `internal/dbrestore`. Restores a Cloud SQL instance to a point
in time as a new instance: a point-in-time clone, which gets the source's settings such as its
tier, network, database flags and labels. The source is never changed.

Serve it with `just serve db-restore`. Setup, calling conventions and the response format are
in the [README](../README.md).

- [Input](#input)
- [How it behaves](#how-it-behaves)
- [Test fixtures](#test-fixtures)
- [Test matrix](#test-matrix)
- [Cleanup](#cleanup)

## Input

| Field | Required | Notes |
|---|---|---|
| `action` | no, but the tool schema requires it | `plan` or `execute`. Defaults to `plan`. |
| `instance` | yes | Source instance name in `GOOGLE_CLOUD_PROJECT`. The recipes default to the fixture, `$P-db`. |
| `targetInstance` | yes | Name of the new instance. It must not exist. |
| `restorePointInTime` | yes | RFC 3339 time to restore to. `just db-point 5` prints five minutes ago. |

```
just db-plan    <targetInstance> <restorePointInTime> [instance]
just db-execute <targetInstance> <restorePointInTime> [instance]
```

## How it behaves

Guards, evaluated in this order:

| Guard | Passes when |
|---|---|
| `source` | The source exists, is `RUNNABLE` and isn't a replica. When it doesn't exist, this is the only guard in the response. |
| `point-in-time-recovery` | Point-in-time recovery is enabled, or, for MySQL, binary logging. |
| `new-instance` | `targetInstance` isn't the source and doesn't exist. |
| `restore-point` | `restorePointInTime` lies between the earliest and latest restore points Cloud SQL reports (`getLatestRecoveryTime`). If it doesn't report the earliest, it is estimated from the transaction log retention. Only evaluated with point-in-time recovery. |

```
plan     -> planned; result.earliestRestorePoint and latestRestorePoint
execute  -> done; submitted true, operation set, target {name, state: PENDING_CREATE}
plan     -> refused, new-instance failed: the target exists; result.target reports its state,
            connectionName, ipAddresses and createTime
```

Cloud SQL can't label a clone when creating it, and labelling it afterwards needs
`cloudsql.instances.update`, which would let the function change any instance. So the function
can't tell its own restore from another instance with that name: once the target exists, it is
reported and refused, never changed. Its `createTime` tells whether it is the restore you
started.

The custom role has `cloudsql.instances.get` and `cloudsql.instances.clone`. Test D1 checks
that `getLatestRecoveryTime` needs nothing more.

## Test fixtures

```bash
just db-fixtures       # PostgreSQL $P-db with point-in-time recovery, and replica $P-db-replica; ~15 minutes
just db-show           # state, type, recovery settings and addresses
```

Wait about 10 minutes after creating it before restoring, so there is a recovery window.
MySQL binary logging, busy sources and a missing earliest restore point are covered by the unit
tests in `internal/dbrestore`. Each restore creates an instance that costs money until
`just db-cleanup`.

## Test matrix

See [Testing a function](../README.md#testing-a-function) for the conventions.

### A. Input validation, no GCP calls or credentials needed

| # | Request | Expected |
|---|---|---|
| A1 | `just db-plan DB_x 2026-10-02T08:00:00Z` | 400 `targetInstance "DB_x" is not a Cloud SQL instance name` |
| A2 | `just db-plan $P-db-r1 yesterday` | 400 `restorePointInTime "yesterday" is not an RFC 3339 timestamp` |

### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just db-plan $P-db-r1 "$(just db-point 5)" nope` | `refused`; only `source` failed, `The instance doesn't exist.` | - |
| B2 | `just db-plan $P-db-r1 "$(just db-point 5)"` | `planned`; all 4 guards pass; `earliestRestorePoint` near the creation time, `latestRestorePoint` near now | `just resources`: no `$P-db-r1` |
| B3 | `just db-plan $P-db-r1 2020-01-01T00:00:00Z` | `refused`; `restore-point` failed | - |
| B4 | `just db-plan $P-db-replica "$(just db-point 5)"` | `refused`; `new-instance` failed, the replica exists; `result.target` describes it | - |
| B5 | `just db-plan $P-db-r1 "$(just db-point 5)" $P-db-replica` | `refused`; `source` failed, `Instance $P-db-replica is a READ_REPLICA_INSTANCE. Restore its primary instead.` | - |

### C. Execute

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just db-execute $P-db-replica "$(just db-point 5)"` | `refused`; nothing submitted | replica unchanged |
| C2 | `just db-execute $P-db-r1 "$(just db-point 5)"` | `done`; `submitted` true, `operation` set, `target.state` `PENDING_CREATE` | `just db-show $P-db-r1`: `PENDING_CREATE`, later `RUNNABLE` |
| C3 | `just db-plan $P-db-r1 "$(just db-point 5)"` | `refused`; `new-instance` failed; `result.target` reports its state and, once `RUNNABLE`, its addresses | - |
| C4 | `just db-execute $P-db-r1 "$(just db-point 5)"` | `refused`; the restore isn't submitted again | - |

### D. Permissions

Run as the `db-restore` service account, see
[Running as the minimal service account](../README.md#running-as-the-minimal-service-account).

| # | Setup | Expected |
|---|---|---|
| D1 | Repeat B2 and C2 with another target, `$P-db-r2` | Same results. **Check that B2 works**: if `getLatestRecoveryTime` returns 403, the role needs the permission it names |
| D2 | `just role-remove db-restore cloudsql.instances.clone`, wait, restart, execute with `$P-db-r3` | 502 `Provider`, 403 on `clone`; nothing created |

Add the permissions back with `just role-add db-restore <permission>`.

## Cleanup

```bash
just db-cleanup         # the fixtures and every $P-db* restore
just sa-delete db-restore
```
