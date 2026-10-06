# volume-delete

Entry point `VolumeDelete`, package `internal/volumedelete`. Deletes an orphaned, unattached
zonal persistent disk that the GCE PD CSI driver created for a PersistentVolumeClaim. It
doesn't support regional disks.

Serve it with `just serve` (`volume-delete` is the default). Setup, calling conventions and the
response format are in the [README](../README.md).

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
| `disk` | yes | Disk name in `GOOGLE_CLOUD_PROJECT`. |
| `pvName` | yes | PersistentVolume the disk was created for. The caller confirms in the cluster that it no longer exists. |
| `snapshot` | no | Defaults to `true`. The function rejects `false` unless `ALLOW_SKIP_SNAPSHOT=true`. |

```
just volume-plan    <disk> <pvName> [snapshot]   # snapshot: omitted, true or false
just volume-execute <disk> <pvName> [snapshot]
```

## How it behaves

Guards, evaluated in this order:

| Guard | Passes when |
|---|---|
| `exists` | The disk exists. When it doesn't, this is the only guard in the response. |
| `unattached` | The disk has no `users`, meaning no instances use it. |
| `state` | The disk status is `READY`. |
| `kubernetes` | The disk description is the CSI driver's JSON and `kubernetes.io/created-for/pv/name` equals `pvName`. |
| `snapshot` | See below. |

The pre-deletion snapshot:

- is named `<disk, max 40 chars>-predelete-<unix time>` and labelled
  `plural-sh-volume-delete=<numeric disk ID>`;
- counts only when its `sourceDiskId` is the disk's ID, it started in the last 24 hours, with
  5 minutes of clock skew tolerated, and it started after the disk's `lastDetachTimestamp`.
  Matching on `sourceDiskId` means a recreated disk with the same name, or a snapshot with a
  copied label, never matches;
- stays as the backup after the disk is deleted. The function never deletes it.

With a snapshot required, deleting takes two executes:

```
plan     -> planned, snapshot guard "execute takes a snapshot first ..."
execute  -> refused, snapshot started (result.snapshot.state = inProgress, result.operation set)
plan     -> refused while the snapshot is in progress, planned once it is completed
execute  -> done, disk deletion submitted (result.deleted = true)
```

If any of `exists`, `unattached`, `state` or `kubernetes` fails, execute refuses and takes no
snapshot.

With `snapshot: false`, which needs `ALLOW_SKIP_SNAPSHOT=true`, the snapshot guard reports
`The caller skipped the snapshot.`, the response has no `result.snapshot` and one execute deletes the disk.

## Test fixtures

The function treats a disk as CSI-created when its description holds the CSI metadata, so you
don't need a cluster. The fixtures are 10 GB `pd-standard` disks and one `e2-micro` VM, which
cost almost nothing.

```bash
just volume-fixtures   # creates everything below and attaches $P-att to $P-vm
just resources         # lists the disks, snapshots and VMs with the prefix
```

| Disk | Description | Used for |
|---|---|---|
| `$P-ok` | CSI metadata for `pvc-e2e-ok` | happy path |
| `$P-att` | CSI metadata for `pvc-e2e-att` | attached to `$P-vm` |
| `$P-det` | CSI metadata for `pvc-e2e-det` | snapshot, then attach and detach |
| `$P-nok8s` | none | not created by Kubernetes |
| `$P-nopv` | PVC name but no PV name | incomplete CSI metadata |
| `$P-skip` | CSI metadata for `pvc-e2e-skip` | `snapshot: false` |
| `$P-other` | CSI metadata for `pvc-e2e-other` | source of a fake labelled snapshot |

Other helpers:

| Recipe | Does |
|---|---|
| `just volume-disk <disk>` | Shows a disk's ID, status, users, last detach time and description. |
| `just volume-snapshots <disk>` | Lists the snapshots labelled with the disk's ID. |
| `just volume-attach <disk>`, `just volume-detach <disk>` | Attach or detach a disk on `$P-vm`. |
| `just volume-recreate <disk> <pvName>` | Creates a disk again with CSI metadata. |

## Test matrix

See [Testing a function](../README.md#testing-a-function) for the conventions. The A cases are
raw bodies for `just invoke`.

### A. Input validation, no GCP calls or credentials needed

| # | Request | Expected |
|---|---|---|
| A1 | `curl -X GET localhost:8080/` | 405, empty body |
| A2 | `{"zone":"europe-central2","disk":"d","pvName":"p"}` | 400 `zone "europe-central2" is not a Compute Engine zone` |
| A3 | `{"zone":"europe-central2-a","disk":"Bad_Disk","pvName":"p"}` | 400 `is not a Compute Engine disk name` |
| A4 | `{"zone":"europe-central2-a","disk":"d","pvName":"P!"}` | 400 `pvName "P!" is not a PersistentVolume name` |
| A5 | `{"zone":"europe-central2-a","disk":"d"}` | 400 ``missing field `pvName` `` |
| A6 | `{"action":"delete",...}` | 400 ``unknown variant `delete`, expected `plan` or `execute` `` |
| A7 | `{"action":null,...}` | 400 `invalid type: null` |
| A8 | `{... ,"snapshot":false}` with `ALLOW_SKIP_SNAPSHOT=false` | 400 `snapshot: false is not allowed by this installation` |
| A9 | `not json` | 400 `InvalidRequest` |
| A10 | Valid body, server started without `GOOGLE_CLOUD_PROJECT` | 502 `Provider`, `GOOGLE_CLOUD_PROJECT is not set` |
| A11 | Body over 2 MiB | 413, empty body |

### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just volume-plan does-not-exist pvc-x` | `refused`; guards = only `exists` failed, `The volume doesn't exist.`; `result` = `{"deleted":false,"snapshot":{"state":"missing"}}` | - |
| B2 | `just volume-plan $P-ok pvc-e2e-ok` | `planned`; all 5 guards pass; `kubernetes` = `The volume was created for PersistentVolume pvc-e2e-ok of PersistentVolumeClaim e2e/data.`; `snapshot` = `execute takes a snapshot first ...`; `result.volume.state` = `READY`, `sizeGib` = 10, `kubernetes` = `{pvc:data, namespace:e2e, pv:pvc-e2e-ok}` | `just volume-snapshots $P-ok` is empty |
| B3 | `just volume-plan $P-att pvc-e2e-att` | `refused`; `unattached` failed, `The volume is attached to $P-vm.`; `result.volume.attachedTo` = `[$P-vm]` | - |
| B4 | `just volume-plan $P-nok8s pvc-x` | `refused`; `kubernetes` failed, `not created by Kubernetes for a PersistentVolumeClaim ...`; no `result.volume.kubernetes` | - |
| B5 | `just volume-plan $P-ok pvc-other` | `refused`; `kubernetes` failed, `created for PersistentVolume pvc-e2e-ok, not pvc-other` | - |
| B6 | `just volume-plan $P-nopv pvc-x` | `refused`; `kubernetes` failed, `created for PersistentVolume <unknown>, not pvc-x` | - |
| B7 | `just zone=europe-central2-z volume-plan $P-ok pvc-e2e-ok`, a valid zone name that doesn't exist | Record the actual result. An API 404 gives `exists` failed, anything else a 502 `Provider` | - |

### C. Execute with snapshot, happy path on `$P-ok`

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just volume-execute $P-ok pvc-e2e-ok` | `refused`; `snapshot` guard failed, `No snapshot has completed yet.`; `result.snapshot` = `{state: inProgress, id: $P-ok-predelete-<ts>}`; `result.operation` set; `deleted` = false | `just volume-snapshots $P-ok` shows 1 snapshot, label = disk ID, `sourceDiskId` = disk ID; disk still exists |
| C2 | Right after C1: `just volume-plan $P-ok pvc-e2e-ok` | while the snapshot is creating: `refused`, `Snapshot <id> is in progress (creating\|uploading). Execute again once it completes.` | still exactly 1 snapshot |
| C3 | Right after C1, and only while C2 still shows the snapshot in progress: `just volume-execute $P-ok pvc-e2e-ok` | `refused`, same in-progress detail; no new snapshot | still exactly 1 snapshot |
| C4 | Once `just volume-snapshots $P-ok` shows `READY`: `just volume-plan $P-ok pvc-e2e-ok` | `planned`; `snapshot` = `Snapshot <id> has completed.`; `result.snapshot.state` = `completed` | - |
| C5 | `just volume-execute $P-ok pvc-e2e-ok` | `done`; `result.deleted` = true; `result.operation` set; `result.snapshot.state` = `completed` | `just volume-disk $P-ok` fails with NotFound after a few seconds; the snapshot still exists |
| C6 | `just volume-plan $P-ok pvc-e2e-ok` | Same as B1 | - |

An empty 10 GB disk usually snapshots in well under a minute, so C2 and C3 race the snapshot.
If you miss the window, C2 shows `planned` with a completed snapshot. **Skip C3 then.** It
would delete the disk.

### D. Unsafe execute takes no snapshot

| # | Request | Expected | Verify |
|---|---|---|---|
| D1 | `just volume-execute $P-att pvc-e2e-att` | `refused`; `unattached` failed; `snapshot` guard failed `No snapshot has completed yet.`; no `result.operation` | `just volume-snapshots $P-att` empty, disk still attached |
| D2 | `just volume-execute $P-nok8s pvc-x` | `refused`; `kubernetes` failed; no `result.operation` | no snapshot of `$P-nok8s` |
| D3 | Before C, or on a recreated `$P-ok`: `just volume-execute $P-ok pvc-other` | `refused`; `kubernetes` failed; no `result.operation` | no new snapshot |

### E. Snapshot matching

| # | Steps | Expected |
|---|---|---|
| E1 | Snapshot before detach is ignored. `just volume-execute $P-det pvc-e2e-det`, wait for `READY`, then `just volume-attach $P-det` and `just volume-detach $P-det`. Then `just volume-plan $P-det pvc-e2e-det`. | `planned`; `snapshot` guard back to `execute takes a snapshot first ...`, `result.snapshot.state` = `missing`; `result.volume.detachedAt` set and later than the snapshot's creation time |
| E2 | Next `just volume-execute $P-det pvc-e2e-det` | `refused`; a second snapshot starts |
| E3 | Recreated disk doesn't reuse the old snapshot. After C5: `just volume-recreate $P-ok pvc-e2e-ok`, then `just volume-plan $P-ok pvc-e2e-ok` | `planned`, `result.snapshot.state` = `missing`, even though C1's snapshot, labelled with the old ID, still exists |
| E4 | Copied label doesn't match. Create a snapshot of `$P-other` carrying `$P-ok`'s label: `gcloud compute snapshots create $P-fake --source-disk $P-other --source-disk-zone $ZONE --labels plural-sh-volume-delete=$(gcloud compute disks describe $P-ok --zone $ZONE --format 'value(id)')`. Then `just volume-plan $P-ok pvc-e2e-ok` | `result.snapshot.state` = `missing`, because `sourceDiskId` differs |

### F. Skipping the snapshot, served with `ALLOW_SKIP_SNAPSHOT=true just serve`

| # | Request | Expected | Verify |
|---|---|---|---|
| F1 | `just volume-plan $P-skip pvc-e2e-skip false` | `planned`; `snapshot` guard `The caller skipped the snapshot.`; no `result.snapshot` | - |
| F2 | `just volume-execute $P-skip pvc-e2e-skip false` | `done`, `deleted` = true, no `result.snapshot` | disk gone, `gcloud compute snapshots list --filter "name~^$P-skip"` empty |
| F3 | `just volume-plan $P-ok pvc-e2e-ok true` | Same as without the field: snapshot required | - |

### G. Permissions and provider errors

Run as the `volume-delete` service account, see
[Running as the minimal service account](../README.md#running-as-the-minimal-service-account).

| # | Setup | Expected |
|---|---|---|
| G1 | Repeat B1 to B3, C1 to C6 and E3 | Same results, so the custom role is enough |
| G2 | Service account without the custom role, `just volume-plan $P-ok pvc-e2e-ok` | 502 `Provider`, `compute API: ... 403 ... compute.disks.get` |
| G3 | `just role-remove volume-delete compute.snapshots.list`, plan on `$P-ok` | 502 `Provider`, 403 mentioning `compute.snapshots.list` |
| G4 | Without `compute.snapshots.setLabels` or `compute.disks.createSnapshot`, execute on a fresh disk | 502 `Provider`, 403; no snapshot created |
| G5 | `GOOGLE_CLOUD_PROJECT` set to a project you can't access | 502 `Provider`, 403 |

Add the permissions back with `just role-add volume-delete <permission>`.

The unit tests in `internal/volume` and `internal/volumedelete` cover states that are hard to
produce on demand: a snapshot older than 24 hours, a snapshot in `FAILED` or `DELETING`, a disk
that isn't `READY`, such as `CREATING` or `RESTORING`, and several snapshots started at the
same time.

## Cleanup

```bash
just volume-cleanup         # test VM, disks and snapshots with the prefix
just sa-delete volume-delete
```
