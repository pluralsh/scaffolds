# Azure functions: manual check against real Azure

Checks how the Azure functions behave against real ARM, beyond what the unit tests' mock covers:
- real response shapes and the tags AKS sets;
- whether providers accept the writes the functions send;
- multi-step executes, load balancer rule health, and the ssh-access expiry sweep.

The functions run locally with `cargo run`, so nothing is deployed. Not covered here, for a
later deployment test: the Functions host on Flex Consumption, the custom roles and ABAC
condition, cold starts, and the Plural path.

**The functions run as you.** `just run-azure` builds them with the `az-cli` feature of `functions-azure`,
which authenticates as the az CLI's account instead of the managed identity (release packages
never have it). They can reach anything you can, so only pass them the fixtures' IDs.

Tools: `az` (logged in to the test subscription), `jq`, `kubectl`, `python3`, `cargo`.

## Setup

```bash
cd functions                          # the recipes work from anywhere below it too
export E2E_PREFIX=cfe2e               # resource groups cfe2e-aks, -node, -vms, -db
just fixtures-azure up                # ~20 min, about $5 a day; E2E_MYSQL=1 adds MySQL, E2E_SSH_PUBLIC=1 a reachable vm-keep
source e2e/azure/.work/fixtures.env   # the IDs used below
```

| Group | What `fixtures.sh` puts there |
|---|---|
| `<p>-aks` | AKS cluster `$CLUSTER_ID`, with pools `system` (1 node), `manual` (User, 1 node) and `auto` (User, autoscaled 0-1) |
| `<p>-node` | the cluster's node resource group, plus the following. The orphaned disk `$ORPHAN_DISK` comes from a PV that was deleted (Retain StorageClass); its PV name is `$ORPHAN_PV`. Disk `$NOTAG_DISK` has no Kubernetes tags; `$ATTACHED_DISK` is tagged and attached to vm-keep. The live Service `e2e/web` has frontend `$LIVE_FRONTEND` on `$LIVE_LB`. `$SYN_LB` has frontends `$FE_ORPHAN` (owned by `e2e/orphan`), `$FE_OTHER` (owned by `e2e/other`) and `$FE_OUTBOUND` (with an outbound rule). `$SYN_LB_LAST` has the single frontend `$FE_LAST` and an empty pool. |
| `<p>-vms` | `$VM_KEEP`: Entra ID login, an `aks-managed-` tag. `$VM_DEL`: plain, with a data disk. |
| `<p>-db` | `$PG_SERVER` (and `$MY_SERVER`), in eastus2 (`E2E_DB_LOCATION`) |

Many checks delete fixtures; `just fixtures-azure up` again recreates what is missing. `just
fixtures-azure` alone shows what exists.

## Running a function

```bash
just run-azure vm-delete "{\"vmId\":\"$VM_DEL\"}"
```

`just run-azure` builds the function and runs it once with that input, as the Functions host would for a
workbench call. It prints the JSON answer, then the function's logs and how long it took (stderr).
Exit code 0 means an answer, whatever its outcome; 2 means an invalid input (the 400 a
workbench would get); 1 means another error (502).

Each function gets the settings terraform would give it: `ALLOW_SKIP_SNAPSHOT=false`,
`MAX_NODE_COUNT=3`, `MAX_DURATION_MINUTES=60`, and `SCOPES` set to the VMs resource group. Set
any of them to override. `action` defaults to `plan`; add `"action":"execute"` to make the change.
Each call should take well under the workbenches' 30 s.

## Checklist

### volume-delete
```bash
just run-azure volume-delete "{\"diskId\":\"$ORPHAN_DISK\",\"pvName\":\"$ORPHAN_PV\"}"
```
- [ ] plan on the orphaned disk → `planned` (the real CSI tags match)
- [ ] wrong `pvName`, or `$NOTAG_DISK` → refused: `kubernetes`
- [ ] `$ATTACHED_DISK` with `pvName` `pvc-e2e-attached` → refused: `state`, `unattached`
- [ ] `"snapshot":false` → exit 2 (invalid input); it works with `ALLOW_SKIP_SNAPSHOT=true just run-azure volume-delete …`
- [ ] a missing disk → refused: `exists`
- [ ] execute → refused: `snapshot` in progress. One snapshot tagged `plural.sh-volume-delete` appears, and the disk stays.
- [ ] executing again before it completes takes no second snapshot
- [ ] executing after it completes → `done`, `deleted: true`. The disk is gone and the snapshot stays.

### lb-frontend-delete
```bash
just run-azure lb-frontend-delete "{\"loadBalancerId\":\"$SYN_LB\",\"frontendName\":\"$FE_ORPHAN\",\"serviceName\":\"e2e/orphan\"}"
```
- [ ] plan → `planned`, step `removeFrontend`. **Watch:** this is the rule health call on an empty
  backend pool. If it errors, real orphans can never be removed.
- [ ] `$FE_OTHER` with `e2e/orphan` → refused: `public-ip-owner`
- [ ] `$FE_OUTBOUND` with `e2e/outbound` → refused: `inbound-only`
- [ ] `$LIVE_FRONTEND` on `$LIVE_LB` with `e2e/web` → refused: `no-healthy-backends`
- [ ] frontend name `kubernetes` → exit 2 (invalid input)
- [ ] execute → the frontend, its rule and its probe are gone. The other frontends, rules and the
  outbound rule are intact, and the public IP stays. **Watch:** Network must accept the full-body PUT.
- [ ] execute again → step `deletePublicIp`; `kubernetes-$FE_ORPHAN` is gone
- [ ] execute once more → exit 0, step `nothing`
- [ ] `$SYN_LB_LAST` / `$FE_LAST` / `e2e/last`, execute twice → the LB is deleted, then its public IP
- [ ] the `e2e/web` Service still has its external IP
- [ ] **Watch:** the call times, since the health polling is the slowest part of any function.

### node-pool-resize
```bash
just run-azure node-pool-resize "{\"clusterId\":\"$CLUSTER_ID\",\"nodePool\":\"manual\",\"count\":2}"
```
- [ ] plan manual 1 → 2 → `planned`
- [ ] pool `auto` → refused: `manually-scaled`
- [ ] pool `system`, count 0 → refused: `count-allowed`
- [ ] count 4 (above 3) → refused: `count-allowed`
- [ ] pool `nope` → refused: `exists`
- [ ] execute 1 → 2 → `done`, `submitted: true`. **Watch:** AKS must accept the full-body PUT with `If-Match`.
- [ ] plan right after → refused: `idle`
- [ ] the pool reaches 2 nodes; executing 2 again → `submitted: false`
- [ ] execute 0 → done; the pool reaches 0

### ssh-access
```bash
just run-azure ssh-access "{\"vmId\":\"$VM_KEEP\",\"principalId\":\"$PRINCIPAL\"}"
```
- [ ] plan → `planned`, with an `az ssh vm` command (or Bastion, with `E2E_BASTION_ID`)
- [ ] `$VM_DEL` → refused: `entra-login`
- [ ] `"durationMinutes":61` → refused: `duration`
- [ ] `principalId` `someone@example.com` → exit 2 (invalid input)
- [ ] execute, 30 min → `granted: true`, and a Virtual Machine User Login assignment on vm-keep with
  description `plural.sh ssh-access expires=…`
- [ ] execute, 5 min → `expiresAt` unchanged, still one assignment
- [ ] `"role":"admin"` → a separate Administrator Login assignment
- [ ] `"revoke":true` → both removed
- [ ] add the User Login role on vm-keep for yourself by hand, then execute → refused:
  `not-assigned`, with `otherAccess` listing it. Remove the manual assignment afterwards.
- [ ] grant 1 minute, wait a minute, then `just run-azure ssh-access-expire` → exit 0, and the assignment is gone
- [ ] optional, with `E2E_SSH_PUBLIC=1` and the `ssh` az extension: grant, then run the returned
  command and log in

### vm-delete
```bash
just run-azure vm-delete "{\"vmId\":\"$VM_DEL\"}"
```
Run this after ssh-access, which uses `$VM_DEL`.
- [ ] `$VM_KEEP` → refused: `not-aks`
- [ ] plan `$VM_DEL` → `planned`; the result lists the OS disk and NIC to delete and the data disk to keep
- [ ] a missing VM → refused: `exists`
- [ ] execute until `deleted: true`. The first execute usually only sets the delete options with
  a PATCH. **Watch:** the DELETE with `If-Match` must be accepted.
- [ ] the VM, `$VM_DEL_OS_DISK` and `$VM_DEL_NIC` are gone; `$VM_DEL_DATA_DISK` is kept, Unattached

### db-restore
```bash
just run-azure db-restore "{\"serverId\":\"$PG_SERVER\",\"targetServerName\":\"$E2E_PREFIX-pg-r1\",\"restorePointInTime\":\"<UTC time>\"}"
```
- [ ] plan with a point between `earliestRestorePoint` (in the result) and now → `planned`
- [ ] target = the source's name → refused: `new-server`
- [ ] a point in the future, or before the earliest → refused: `restore-point`
- [ ] target `Bad_Name` → exit 2 (invalid input)
- [ ] execute → `submitted: true`, and the target server appears. **Watch:** ARM must accept the PUT with `If-None-Match: *`.
- [ ] execute again → `done`, `submitted: false`
- [ ] the same target with another point → refused: `new-server`
- [ ] the restored server becomes Ready (10-30 min)
- [ ] the same with `$MY_SERVER`, if created

## Cleanup

```bash
just fixtures-azure down   # only deletes resource groups tagged plural-e2e=<prefix>
```
