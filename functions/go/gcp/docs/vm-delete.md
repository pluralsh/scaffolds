# vm-delete

Entry point `VMDelete`, package `internal/vmdelete`. Deletes a standalone Compute Engine
instance together with its boot disk. Data disks are detached and kept, so delete them with
[volume-delete](volume-delete.md) later if needed. Local SSDs are always deleted with the
instance.

Serve it with `just serve vm-delete`. Setup, calling conventions and the response format are
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
| `zone` | yes | For example `europe-central2-a`. |
| `instance` | yes | Instance name in `GOOGLE_CLOUD_PROJECT`. |

```
just vm-plan    <instance>
just vm-execute <instance>
```

## How it behaves

Guards, evaluated in this order:

| Guard | Passes when |
|---|---|
| `exists` | The instance exists. When it doesn't, this is the only guard in the response. |
| `standalone` | No managed instance group created it: no `created-by` metadata pointing at an instance group manager. |
| `not-gke` | It has neither the `goog-gke-node` nor the `goog-k8s-cluster-name` label. |
| `deletion-protection` | Deletion protection is off. The function never turns it off. |
| `boot-disk` | It has a persistent boot disk. |
| `idle` | Its status is `RUNNING`, `TERMINATED` (stopped) or `SUSPENDED`. |
| `auto-delete` | Only added to an execute that just changed the disks' auto-delete flags and found them not applied yet. |

Compute Engine deletes the attached disks whose auto-delete flag is set. Execute sets it on the
boot disk and clears it on the data disks, then deletes the instance:

```
plan     -> planned, result.autoDeleteSet false, lists bootDisk, dataDisks and localSsds
execute  -> sets the flags and reads the instance again
              flags applied: done, deletion submitted
              not yet:       refused, auto-delete guard "execute again once it has finished"
execute  -> done, result.deleted true, result.operation set
```

If any guard fails, execute refuses and changes nothing. Once `result.autoDeleteSet` is true,
deleting the instance in any way deletes its boot disk and keeps its data disks.

The custom role has `compute.instances.get`, `.delete`, `.setDiskAutoDelete` and
`compute.disks.update`, which changing the flag requires. Test D1 checks whether deleting the
boot disk with the instance also needs `compute.disks.delete`; the GCP docs don't say.

## Test fixtures

```bash
just vm-fixtures         # creates the instances below; takes a few minutes
just vm-show $P-vmd-ok   # status, deletion protection, disks with their auto-delete flags, labels, metadata
just vm-mig-instance     # name of the instance in the managed instance group
```

| Instance | Set up as | Used for |
|---|---|---|
| `$P-vmd-ok` | boot disk auto-delete off, data disk `$P-vmd-ok-data` auto-delete on | happy path, the flags are the opposite of what deleting needs |
| `$P-vmd-prot` | deletion protection on | refused |
| `$P-vmd-stopped` | stopped, default flags | deleting a stopped instance in one execute |
| `$(just vm-mig-instance)` | in managed instance group `$P-vmd-mig` from template `$P-vmd-tpl` | refused |

GKE nodes, local SSDs, instances without a boot disk and instances in transitional states are
covered by the unit tests in `internal/vmdelete`.

## Test matrix

See [Testing a function](../README.md#testing-a-function) for the conventions.

### A. Input validation, no GCP calls or credentials needed

| # | Request | Expected |
|---|---|---|
| A1 | `just vm-plan Bad_VM` | 400 `instance "Bad_VM" is not a Compute Engine instance name` |
| A2 | `just zone=europe-central2 vm-plan vm-1` | 400 `zone "europe-central2" is not a Compute Engine zone` |
| A3 | `just invoke '{"action":"plan","zone":"europe-central2-a"}'` | 400 ``missing field `instance` `` |

### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just vm-plan does-not-exist` | `refused`; only `exists` failed, `The instance doesn't exist.`; `result` = `{"autoDeleteSet":false,"deleted":false}` | - |
| B2 | `just vm-plan $P-vmd-ok` | `planned`; all 6 guards pass; `result.instance` = `{bootDisk: $P-vmd-ok, dataDisks: [$P-vmd-ok-data], state: RUNNING}`; `autoDeleteSet` false | `just vm-show $P-vmd-ok`: boot disk auto-delete still false, data disk still true |
| B3 | `just vm-plan $P-vmd-prot` | `refused`; `deletion-protection` failed, `Deletion protection is enabled. Disable it first.` | - |
| B4 | `just vm-plan "$(just vm-mig-instance)"` | `refused`; `standalone` failed, `Managed instance group $P-vmd-mig created the instance. ...`; `result.instance.instanceGroupManager` = `$P-vmd-mig` | - |
| B5 | `just vm-plan $P-vmd-stopped` | `planned`; `idle` = `The instance's status is TERMINATED.`; `autoDeleteSet` true (default flags, no data disk) | - |
| B6 | Optional, if the project has a GKE cluster: `just zone=<node zone> vm-plan <GKE node>` | `refused`; `standalone` and `not-gke` failed | - |

### C. Execute

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just vm-execute $P-vmd-prot` | `refused`; `deletion-protection` failed; no `result.operation` | `just vm-show $P-vmd-prot`: instance and flags unchanged |
| C2 | `just vm-execute "$(just vm-mig-instance)"` | `refused`; `standalone` failed | the instance still exists, flags unchanged |
| C3 | `just vm-execute $P-vmd-ok` | Usually `refused` with `auto-delete` failed; `result.autoDeleteSet` true, `deleted` false. If Compute Engine applied the flags at once: `done`, as in C4 | After a few seconds, `just vm-show $P-vmd-ok`: boot disk auto-delete true, data disk false |
| C4 | `just vm-execute $P-vmd-ok` | `done`; `result.deleted` true, `result.operation` set | After a minute, `just resources`: instance and boot disk `$P-vmd-ok` gone, `$P-vmd-ok-data` still there |
| C5 | `just vm-plan $P-vmd-ok` | Same as B1 | - |
| C6 | `just vm-execute $P-vmd-stopped` | `done` in one execute; no `auto-delete` guard | Instance and its boot disk gone |

### D. Permissions

Recreate the fixtures first (`just vm-cleanup && just vm-fixtures`), then run as the
`vm-delete` service account, see
[Running as the minimal service account](../README.md#running-as-the-minimal-service-account).

| # | Setup | Expected |
|---|---|---|
| D1 | Repeat B2 to B5 and C1 to C4 | Same results. **Check that C4 also deletes the boot disk**: the role has no `compute.disks.delete`. If the boot disk stays, the role needs it |
| D2 | `just role-remove vm-delete compute.disks.update`, wait, restart, execute on a fresh `$P-vmd-ok` | 502 `Provider`, 403 on `setDiskAutoDelete`; flags unchanged |
| D3 | `just role-remove vm-delete compute.instances.get`, wait, restart, plan on `$P-vmd-ok` | 502 `Provider`, 403 mentioning `compute.instances.get` |

Add the permissions back with `just role-add vm-delete <permission>`.

## Cleanup

```bash
just vm-cleanup          # instances, group, template and the disks they kept
just sa-delete vm-delete
```
