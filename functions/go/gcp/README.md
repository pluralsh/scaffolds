# GCP functions in Go

Operational functions that run as Cloud Run functions, the 2nd generation of Cloud Functions.
Every function is a [Functions Framework](https://github.com/GoogleCloudPlatform/functions-framework-go)
entry point registered in `functions.go`. Production deploys go through
`catalogs/infra/cloud-functions/terraform/gcp`.

This README shows how to run a function locally against a real GCP project and test its logic
end to end. `VolumeDelete` is the only function so far. The `Justfile` here wraps every step;
run `just` to list the recipes.

- [Prerequisites](#prerequisites)
- [Project setup](#project-setup)
- [Running locally](#running-locally)
- [VolumeDelete](#volumedelete)
  - [How it behaves](#how-it-behaves)
  - [Test fixtures](#test-fixtures)
  - [Test matrix](#test-matrix)
- [Running as the minimal service account](#running-as-the-minimal-service-account)
- [Cleanup](#cleanup)
- [Troubleshooting](#troubleshooting)

## Prerequisites

| Tool | Version | Used for |
|---|---|---|
| Go | 1.27, from `go.mod` | building and running the function |
| `gcloud` | recent | auth, fixtures, verification |
| `just` | 1.40+ | the recipes in `Justfile` |
| `curl`, `jq` | any | calling the function, reading responses |
| `golangci-lint` | v2 | `just check` only |

You need a GCP project where you can create and delete disks, snapshots and a small VM. Use a
sandbox project. The function deletes real disks.

## Project setup

Export these in every shell you test from:

```bash
export PROJECT=my-sandbox-project   # optional, defaults to `gcloud config get project`
export ZONE=europe-central2-a       # Warsaw has zones a, b and c
export P=e2e-vd                     # prefix of every test resource
export PORT=8080                    # port of the local function

just apis                           # enables compute and iam
```

The recipes fall back to the values above when a variable is unset, but the test matrix also
uses `$P` and `$ZONE` in its commands, e.g. `just plan $P-ok pvc-e2e-ok`. Your shell expands
those before `just` runs, so an unset `P` sends the disk name `-ok`. Export `P` and `ZONE`
even when you keep the defaults.

To override a recipe variable for one command, use `just zone=europe-central2-b fixtures`.
This doesn't change `$ZONE` in your shell.

The function authenticates with Application Default Credentials, ADC for short. On Cloud Run,
ADC resolves to the function's service account. Locally, log in as yourself:

```bash
just login-user
```

Your account usually has far more permissions than the function gets. Run the matrix once as
yourself to test the logic, then again as the minimal service account to test the IAM role.
[Running as the minimal service account](#running-as-the-minimal-service-account) has the setup.

## Running locally

From this directory:

```bash
just check    # gofmt, vet, golangci-lint and unit tests
just test     # unit tests only. They cover cases that are hard to reproduce live.
just serve    # serves VolumeDelete on $PORT, in a terminal of its own
```

`just serve` runs `go run ./cmd` with these variables set:

| Variable | Required | Meaning |
|---|---|---|
| `FUNCTION_TARGET` | yes | Entry point to serve at `/`. Without it, each function is at `/<EntryPoint>`, for example `/VolumeDelete`. |
| `GOOGLE_CLOUD_PROJECT` | yes | Project the function acts in. Terraform sets it to the installation's project. |
| `ALLOW_SKIP_SNAPSHOT` | no | `true` lets callers pass `snapshot: false`. Terraform sets it from `allow_skip_snapshot`, which defaults to `false`. |
| `PORT` | no | Listen port. Defaults to `8080`. |
| `GCP_COMPUTE_ENDPOINT` | no | Overrides the Compute Engine API host, for example to point at a fake server. Leave it unset for real GCP. |
| `GOOGLE_APPLICATION_CREDENTIALS` | no | ADC file to use instead of the gcloud default. |

The function creates its Compute Engine client on the first request and reuses it. **Restart
the server after you change credentials or `GOOGLE_CLOUD_PROJECT`.**

Logs go to stdout as JSON lines with `severity` and `message` fields, the format Cloud Logging
parses.

### Calling it

The workbench sends the tool input as the body of a `POST` and reads the JSON response. The
recipes do the same and print the request, the status code and the pretty-printed body:

```
just plan    <disk> <pvName> [snapshot]   # snapshot: omitted, true or false
just execute <disk> <pvName> [snapshot]
just invoke  '<raw JSON body>'           # anything else, e.g. invalid input
```

```bash
just plan does-not-exist pvc-x
```

`plan` and `execute` send `zone` from `$ZONE`.

### Response envelope

Success, HTTP 200:

```json
{
  "action": "plan | execute",
  "outcome": "planned | refused | done",
  "guards": [{ "name": "exists", "passed": true, "detail": "found ..." }],
  "result": { }
}
```

- `planned` means a plan where every guard passed. Nothing changed.
- `refused` means at least one guard failed. A refused execute may still have started a
  snapshot, which `result` reports.
- `done` means the function submitted the change. It doesn't wait for the change to finish.

Failure:

```json
{ "errorType": "InvalidRequest | Provider", "errorMessage": "..." }
```

`InvalidRequest` returns HTTP 400. `Provider` covers GCP API errors and missing configuration
and returns HTTP 502. A method other than `POST` gets 405 and a body over 2 MiB gets 413, both
with an empty body. Response keys are sorted, so bodies match the Rust functions on AWS and
Azure byte for byte.

## VolumeDelete

Deletes an orphaned, unattached zonal persistent disk that the GCE PD CSI driver created for a
PersistentVolumeClaim. It doesn't support regional disks.

### Input

| Field | Required | Notes |
|---|---|---|
| `action` | no, but the tool schema requires it | `plan` or `execute`. Defaults to `plan`. |
| `zone` | yes | For example `europe-central2-a`. |
| `disk` | yes | Disk name in `GOOGLE_CLOUD_PROJECT`. |
| `pvName` | yes | PersistentVolume the disk was created for. The caller confirms in the cluster that it no longer exists. |
| `snapshot` | no | Defaults to `true`. The function rejects `false` unless `ALLOW_SKIP_SNAPSHOT=true`. |

The function ignores unknown fields. Field names are case-sensitive.

### How it behaves

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
`not requested`, the response has no `result.snapshot` and one execute deletes the disk.

### Test fixtures

The function treats a disk as CSI-created when its description holds the CSI metadata, so you
don't need a cluster. The fixtures are 10 GB `pd-standard` disks and one `e2-micro` VM, which
cost almost nothing.

```bash
just fixtures    # creates everything below and attaches $P-att to $P-vm
just resources   # lists the disks, snapshots and VM with the prefix
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

Other helpers: `just disk <disk>` shows a disk's ID, status, users, last detach time and
description, `just snapshots <disk>` lists the snapshots labelled with its ID, `just attach
<disk>` and `just detach <disk>` use the test VM, and `just recreate <disk> <pvName>` creates a
disk again with CSI metadata.

### Test matrix

Run the cases in each group in order, since some build on the one before. The Verify column is
a separate `gcloud` check of whether the function changed GCP. The A cases are raw bodies
for `just invoke`. The other cases need `P` and `ZONE` exported, see
[Project setup](#project-setup).

#### A. Input validation, no GCP calls or credentials needed

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

#### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just plan does-not-exist pvc-x` | `refused`; guards = only `exists` failed, `volume not found`; `result` = `{"deleted":false,"snapshot":{"state":"missing"}}` | - |
| B2 | `just plan $P-ok pvc-e2e-ok` | `planned`; all 5 guards pass; `kubernetes` = `created for PersistentVolume pvc-e2e-ok of PVC e2e/data`; `snapshot` = `execute takes a snapshot first ...`; `result.volume.state` = `READY`, `sizeGib` = 10, `kubernetes` = `{pvc:data, namespace:e2e, pv:pvc-e2e-ok}` | `just snapshots $P-ok` is empty |
| B3 | `just plan $P-att pvc-e2e-att` | `refused`; `unattached` failed, `attached to $P-vm`; `result.volume.attachedTo` = `[$P-vm]` | - |
| B4 | `just plan $P-nok8s pvc-x` | `refused`; `kubernetes` failed, `not created by Kubernetes for a PersistentVolumeClaim ...`; no `result.volume.kubernetes` | - |
| B5 | `just plan $P-ok pvc-other` | `refused`; `kubernetes` failed, `created for PersistentVolume pvc-e2e-ok, not pvc-other` | - |
| B6 | `just plan $P-nopv pvc-x` | `refused`; `kubernetes` failed, `created for PersistentVolume <unknown>, not pvc-x` | - |
| B7 | `just zone=europe-central2-z plan $P-ok pvc-e2e-ok`, a valid zone name that doesn't exist | Record the actual result. An API 404 gives `exists` failed, anything else a 502 `Provider` | - |

#### C. Execute with snapshot, happy path on `$P-ok`

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just execute $P-ok pvc-e2e-ok` | `refused`; `snapshot` guard failed, `no completed snapshot yet`; `result.snapshot` = `{state: inProgress, id: $P-ok-predelete-<ts>}`; `result.operation` set; `deleted` = false | `just snapshots $P-ok` shows 1 snapshot, label = disk ID, `sourceDiskId` = disk ID; disk still exists |
| C2 | Right after C1: `just plan $P-ok pvc-e2e-ok` | while the snapshot is creating: `refused`, `snapshot <id> is in progress (creating\|uploading); execute again once it completes` | still exactly 1 snapshot |
| C3 | Right after C1, and only while C2 still shows the snapshot in progress: `just execute $P-ok pvc-e2e-ok` | `refused`, same in-progress detail; no new snapshot | still exactly 1 snapshot |
| C4 | Once `just snapshots $P-ok` shows `READY`: `just plan $P-ok pvc-e2e-ok` | `planned`; `snapshot` = `snapshot <id> completed`; `result.snapshot.state` = `completed` | - |
| C5 | `just execute $P-ok pvc-e2e-ok` | `done`; `result.deleted` = true; `result.operation` set; `result.snapshot.state` = `completed` | `just disk $P-ok` fails with NotFound after a few seconds; the snapshot still exists |
| C6 | `just plan $P-ok pvc-e2e-ok` | Same as B1 | - |

An empty 10 GB disk usually snapshots in well under a minute, so C2 and C3 race the snapshot.
If you miss the window, C2 shows `planned` with a completed snapshot. **Skip C3 then.** It
would delete the disk.

#### D. Unsafe execute takes no snapshot

| # | Request | Expected | Verify |
|---|---|---|---|
| D1 | `just execute $P-att pvc-e2e-att` | `refused`; `unattached` failed; `snapshot` guard failed `no completed snapshot yet`; no `result.operation` | `just snapshots $P-att` empty, disk still attached |
| D2 | `just execute $P-nok8s pvc-x` | `refused`; `kubernetes` failed; no `result.operation` | no snapshot of `$P-nok8s` |
| D3 | Before C, or on a recreated `$P-ok`: `just execute $P-ok pvc-other` | `refused`; `kubernetes` failed; no `result.operation` | no new snapshot |

#### E. Snapshot matching

| # | Steps | Expected |
|---|---|---|
| E1 | Snapshot before detach is ignored. `just execute $P-det pvc-e2e-det`, wait for `READY`, then `just attach $P-det` and `just detach $P-det`. Then `just plan $P-det pvc-e2e-det`. | `planned`; `snapshot` guard back to `execute takes a snapshot first ...`, `result.snapshot.state` = `missing`; `result.volume.detachedAt` set and later than the snapshot's creation time |
| E2 | Next `just execute $P-det pvc-e2e-det` | `refused`; a second snapshot starts |
| E3 | Recreated disk doesn't reuse the old snapshot. After C5: `just recreate $P-ok pvc-e2e-ok`, then `just plan $P-ok pvc-e2e-ok` | `planned`, `result.snapshot.state` = `missing`, even though C1's snapshot, labelled with the old ID, still exists |
| E4 | Copied label doesn't match. Create a snapshot of `$P-other` carrying `$P-ok`'s label: `gcloud compute snapshots create $P-fake --source-disk $P-other --source-disk-zone $ZONE --labels plural-sh-volume-delete=$(gcloud compute disks describe $P-ok --zone $ZONE --format 'value(id)')`. Then `just plan $P-ok pvc-e2e-ok` | `result.snapshot.state` = `missing`, because `sourceDiskId` differs |

#### F. Skipping the snapshot, served with `ALLOW_SKIP_SNAPSHOT=true just serve`

| # | Request | Expected | Verify |
|---|---|---|---|
| F1 | `just plan $P-skip pvc-e2e-skip false` | `planned`; `snapshot` guard `not requested`; no `result.snapshot` | - |
| F2 | `just execute $P-skip pvc-e2e-skip false` | `done`, `deleted` = true, no `result.snapshot` | disk gone, `gcloud compute snapshots list --filter "name~^$P-skip"` empty |
| F3 | `just plan $P-ok pvc-e2e-ok true` | Same as without the field: snapshot required | - |

#### G. Permissions and provider errors

| # | Setup | Expected |
|---|---|---|
| G1 | Run as the minimal service account from the next section and repeat B1 to B3, C1 to C6 and E3 | Same results, so the custom role is enough |
| G2 | Run as the service account without the custom role, `just plan $P-ok pvc-e2e-ok` | 502 `Provider`, `compute API: ... 403 ... compute.disks.get` |
| G3 | Role without `compute.snapshots.list`, plan on `$P-ok` | 502 `Provider`, 403 mentioning `compute.snapshots.list` |
| G4 | Role without `compute.snapshots.setLabels` or `compute.disks.createSnapshot`, execute on a fresh disk | 502 `Provider`, 403; no snapshot created |
| G5 | `GOOGLE_CLOUD_PROJECT` set to a project you can't access | 502 `Provider`, 403 |

The unit tests in `internal/volume` and `internal/volumedelete` cover states that are hard to
produce on demand: a snapshot older than 24 hours, a snapshot in `FAILED` or `DELETING`, a disk
that isn't `READY`, such as `CREATING` or `RESTORING`, and several snapshots started at the
same time.

## Running as the minimal service account

Terraform gives each function its own service account with a project custom role holding
only the permissions listed in `catalog` in
`catalogs/infra/cloud-functions/terraform/gcp/locals.tf`. The Justfile copies that list into
`volume_delete_permissions`, so keep the two in sync. Reproduce the setup locally:

```bash
just sa-create   # service account $P-fn, custom role $ROLE (default e2eVolumeDelete),
                 # and the right for you to impersonate the service account
just login-sa    # ADC now impersonates $P-fn. Restart `just serve`.
```

IAM changes can take a minute or two to apply. For G3 and G4, run `just role-remove
<permission>`, wait, restart `just serve`, and run `just role-add <permission>` afterwards.
Custom role IDs can't be reused for 7 days after deletion; set `ROLE` to a new ID if
`sa-create` fails on the role.

Switch back to your own credentials with `just login-user`.

## Cleanup

```bash
just cleanup     # VM, disks and snapshots with the prefix
just sa-delete   # service account, its binding and the custom role
```

## Troubleshooting

| Symptom | Cause |
|---|---|
| 502 `GOOGLE_CLOUD_PROJECT is not set` | The variable was missing when the server started. |
| 502 `could not find default credentials` | No ADC. Run `just login-user`. |
| Credential or project change has no effect | The client is cached. Restart the server. |
| 502 with `403` and a permission name | The identity lacks that permission. Compare with `locals.tf`. |
| 502 `API has not been used in project` | Run `gcloud services enable compute.googleapis.com`. |
| 404 from curl | Without `FUNCTION_TARGET`, the function is at `/VolumeDelete`, not `/`. |
| `kubernetes` guard fails on a real CSI disk | `pvName` must equal `kubernetes.io/created-for/pv/name` in `gcloud compute disks describe <disk> --format 'value(description)'`. |
