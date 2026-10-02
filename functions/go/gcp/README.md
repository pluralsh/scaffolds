# GCP functions in Go

Operational functions that run as Cloud Run functions, the 2nd generation of Cloud Functions.
Every function is a [Functions Framework](https://github.com/GoogleCloudPlatform/functions-framework-go)
entry point registered in `functions.go`. Production deploys go through
`catalogs/infra/cloud-functions-gcp/terraform`.

This README covers what all functions share: running one locally against a real GCP project,
calling it, and testing it end to end. Each function's behaviour, fixtures and test matrix are
in `docs/`. The `Justfile` wraps every step; run `just` to list the recipes.

| Function | Entry point | Package | Docs |
|---|---|---|---|
| `volume-delete` | `VolumeDelete` | `internal/volumedelete` | [docs/volume-delete.md](docs/volume-delete.md) |
| `vm-delete` | `VMDelete` | `internal/vmdelete` | [docs/vm-delete.md](docs/vm-delete.md) |
| `node-pool-resize` | `NodePoolResize` | `internal/nodepoolresize` | [docs/node-pool-resize.md](docs/node-pool-resize.md) |
| `lb-frontend-delete` | `LBFrontendDelete` | `internal/lbfrontenddelete` | [docs/lb-frontend-delete.md](docs/lb-frontend-delete.md) |
| `db-restore` | `DBRestore` | `internal/dbrestore` | [docs/db-restore.md](docs/db-restore.md) |
| `ssh-access` | `SSHAccess` | `internal/sshaccess` | [docs/ssh-access.md](docs/ssh-access.md) |

The function key is the one in the terraform catalog (`local.catalog` in
`catalogs/infra/cloud-functions-gcp/terraform/locals.tf`), and the name of the file defining
it there, e.g. `vm-delete.tf`. Recipes that act on a function take it, e.g.
`just serve vm-delete`.

- [Layout](#layout)
- [Prerequisites](#prerequisites)
- [Project setup](#project-setup)
- [Running locally](#running-locally)
- [Testing a function](#testing-a-function)
- [Running as the minimal service account](#running-as-the-minimal-service-account)
- [Adding a function](#adding-a-function)
- [Troubleshooting](#troubleshooting)

## Layout

```
functions.go           registers every entry point with the Functions Framework
cmd/                   local server (`just serve`); Cloud Run builds from the module root instead
internal/core          request/response envelope, guards, errors, the HTTP server and logging
internal/gcp           what the API clients share: project, endpoint overrides, lazily created clients
internal/compute       Compute Engine client: interfaces, SDK implementation, connector
internal/gke           GKE client (clusters, node pool sizes)
internal/cloudsql      Cloud SQL Admin client (instances, recovery window, clones)
internal/iap           IAP client (IAM policies of TCP tunnel resources)
internal/iam           IAM policies as the functions edit them, independent of the API serving them
internal/volume        cloud-agnostic volume deletion rules, shared with the other clouds
internal/<function>    one package per function
docs/<function>.md     behaviour, fixtures and test matrix per function
Justfile               dev, gcp and per-function recipes
```

## Prerequisites

| Tool | Version | Used for |
|---|---|---|
| Go | 1.27, from `go.mod` | building and running the functions |
| `gcloud` | recent | auth, fixtures, verification |
| `just` | 1.40+ | the recipes in `Justfile` |
| `curl`, `jq` | any | calling the functions, reading responses |
| `golangci-lint` | v2 | `just check` only |

You need a GCP project where you can create and delete disks, snapshots, instances and
instance groups, and, for the functions that need them, GKE clusters, load balancer resources,
Cloud SQL instances and IAM policies. Use a sandbox project. The functions delete real
resources, and the GKE and Cloud SQL fixtures cost money while they exist.

## Project setup

Export these in every shell you test from:

```bash
export PROJECT=my-sandbox-project   # optional, defaults to `gcloud config get project`
export ZONE=europe-central2-a       # Warsaw has zones a, b and c
export P=e2e-vd                     # prefix of every test resource
export PORT=8080                    # port of the local function

just apis                           # enables compute, iam, container, sqladmin and iap
```

The recipes fall back to the values above when a variable is unset, but the test matrices also
use `$P` and `$ZONE` in their commands, e.g. `just volume-plan $P-ok pvc-e2e-ok`. Your shell
expands those before `just` runs, so an unset `P` sends the disk name `-ok`. Export `P` and
`ZONE` even when you keep the defaults.

To override a recipe variable for one command, use `just zone=europe-central2-b vm-plan vm-1`.
This doesn't change `$ZONE` in your shell.

The functions authenticate with Application Default Credentials, ADC for short. On Cloud Run,
ADC resolves to the function's service account. Locally, log in as yourself:

```bash
just login-user
```

## Running locally

From this directory:

```bash
just check              # gofmt, vet, golangci-lint and unit tests
just test               # unit tests only. They cover cases that are hard to reproduce live.
just serve vm-delete    # serves one function on $PORT, in a terminal of its own
```

`just serve` runs `go run ./cmd` with these variables set:

| Variable | Required | Meaning |
|---|---|---|
| `FUNCTION_TARGET` | yes | Entry point to serve at `/`. Without it, each function is at `/<EntryPoint>`, for example `/VMDelete`. |
| `GOOGLE_CLOUD_PROJECT` | yes | Project the function acts in. Terraform sets it to the installation's project. |
| `ALLOW_SKIP_SNAPSHOT` | no | volume-delete only. `true` lets callers pass `snapshot: false`. Terraform sets it from `allow_skip_snapshot`, which defaults to `false`. |
| `MAX_NODE_COUNT` | no | node-pool-resize only. Largest total node count it sets. Terraform sets it from `node_pool_max_count` (default 100); `just serve` passes 100 unless set. |
| `MAX_DURATION_MINUTES` | no | ssh-access only. Longest access it grants. Terraform sets it from `ssh_access_max_minutes` (default 240); `just serve` passes 240 unless set. |
| `PORT` | no | Listen port. Defaults to `8080`. |
| `GCP_COMPUTE_ENDPOINT`, `GCP_CONTAINER_ENDPOINT`, `GCP_SQLADMIN_ENDPOINT`, `GCP_IAP_ENDPOINT` | no | Override the Compute Engine, GKE, Cloud SQL Admin and IAP API hosts, for example to point at a fake server. Leave them unset for real GCP. |
| `GOOGLE_APPLICATION_CREDENTIALS` | no | ADC file to use instead of the gcloud default. |

A function creates its API clients on the first request and reuses them. **Restart
the server after you change credentials or `GOOGLE_CLOUD_PROJECT`.**

Logs go to stdout as JSON lines with `severity` and `message` fields, the format Cloud Logging
parses.

### Calling a function

The workbench sends the tool input as the body of a `POST` and reads the JSON response. The
recipes do the same and print the request, the status code and the pretty-printed body. Each
function has `<prefix>-plan` and `<prefix>-execute` recipes that fill in `zone` from `$ZONE`,
listed in its doc. Anything else, such as invalid input, goes through `just invoke`:

```bash
just invoke '{"action":"plan","zone":"europe-central2-a","instance":"vm-1"}'
```

### Response envelope

Success, HTTP 200:

```json
{
  "action": "plan | execute",
  "outcome": "planned | refused | done",
  "guards": [{ "name": "exists", "passed": true, "detail": "..." }],
  "result": { }
}
```

- `planned` means a plan where every guard passed. Nothing changed.
- `refused` means at least one guard failed. A refused execute may still have made a
  preparatory change, such as starting a snapshot or setting auto-delete flags, which `result`
  reports.
- `done` means the function submitted the change. It doesn't wait for the change to finish.

Failure:

```json
{ "errorType": "InvalidRequest | Provider", "errorMessage": "..." }
```

`InvalidRequest` returns HTTP 400. `Provider` covers GCP API errors and missing configuration
and returns HTTP 502. A method other than `POST` gets 405 and a body over 2 MiB gets 413, both
with an empty body. Unknown fields are ignored and field names are case-sensitive. Response
keys are sorted, so bodies match the Rust functions on AWS and Azure byte for byte.

## Testing a function

Every function doc has a test matrix. For each one:

1. Create its fixtures with `just <prefix>-fixtures` and serve it with `just serve <function>`.
2. Run the matrix as yourself. Your account usually has far more permissions than the function
   gets, so this tests the logic.
3. Run its permissions group as the function's own service account, which tests the IAM role.
   See [Running as the minimal service account](#running-as-the-minimal-service-account).
4. Clean up with the recipes at the end of the doc.

Conventions in the matrices:

- Run the cases in each group in order, since some build on the one before.
- `P` and `ZONE` must be exported, see [Project setup](#project-setup).
- The Verify column is a separate `gcloud` check, through a recipe, of whether the function
  changed GCP.
- `just resources` lists everything with the prefix.

## Running as the minimal service account

Terraform gives each function its own service account with a project custom role holding
only the `permissions` in its `catalogs/infra/cloud-functions-gcp/terraform/<function>.tf`.
The Justfile copies those lists into `<function>_permissions` variables, so keep them in
sync. The service account recipes
take the function key, `volume-delete` by default:

```bash
just sa-create vm-delete   # service account $P-vm-delete, custom role e2e_vm_delete,
                           # and the right for you to impersonate the service account
just login-sa vm-delete    # ADC now impersonates it. Restart `just serve`.
```

IAM changes can take a minute or two to apply. For the permission cases, run
`just role-remove <function> <permission>`, wait, restart `just serve`, and run
`just role-add <function> <permission>` afterwards. Custom role IDs can't be reused for 7
days after deletion, so recreating a role you just deleted fails; use another `P` prefix
meanwhile.

Switch back to your own credentials with `just login-user`, and delete the service account
with `just sa-delete <function>`.

## Adding a function

1. Write the handler in `internal/<function>`, implementing `core.Handler`, with unit tests.
   Add the Compute Engine calls it needs to `internal/compute`, behind an interface of their
   own. Other APIs get a client package like `internal/gke`, built on the Google API client
   with a connector from `internal/gcp`.
2. Register the entry point in `functions.go`. CI checks that every `entry_point` in the
   terraform catalog is registered there.
3. In `catalogs/infra/cloud-functions-gcp/terraform`:
   - add `<function>.tf` defining `local.<function>`: entry point, tool description, memory,
     timeout, environment, minimal permissions, the APIs it calls and schema, plus any
     variables only it uses;
   - add its tool schema to `schemas/<function>.json`;
   - register it in `local.catalog` in `locals.tf`, and add it to the `functions` default in
     `variables.tf` and to the GCP list in the catalog's `stack.yaml`.

   `modules/function` deploys it: the Cloud Run function and workbench tool in `main.tf`, its
   service account, custom role and invoker binding in `iam.tf`. Shared resources are in
   `build.tf` (APIs, Artifact Registry repository, source bucket) and `iam.tf` (the build
   service account and its grants). Then regenerate the contract outputs with
   `plural pr contracts --file test/contracts.yaml` from the repo root.
4. In the `Justfile`: add the key to `functions`, map it to the entry point in `serve`, add
   `<function>_permissions` and its case in `sa-create`, and add a group with plan, execute,
   fixtures and cleanup recipes.
5. Write `docs/<function>.md` with its input, behaviour, fixtures and test matrix, link it in
   the table above, and document it in the catalog README.

## Troubleshooting

| Symptom | Cause |
|---|---|
| 502 `GOOGLE_CLOUD_PROJECT is not set` | The variable was missing when the server started. |
| 502 `could not find default credentials` | No ADC. Run `just login-user`. |
| Credential or project change has no effect | The client is cached. Restart the server. |
| 502 with `403` and a permission name | The identity lacks that permission. Compare with the function's `<function>.tf` in the terraform. |
| 502 `API has not been used in project` | Run `just apis`. |
| 404 from curl | Without `FUNCTION_TARGET`, each function is at `/<EntryPoint>`, e.g. `/VMDelete`, not `/`. |
| `kubernetes` guard of volume-delete fails on a real CSI disk | `pvName` must equal `kubernetes.io/created-for/pv/name` in `just volume-disk <disk>`. |
| `plural pr contracts` fails with a Liquid error on binary data | A `.terraform/` directory in the catalog's terraform folder; run `terraform validate` in a copy instead. |
