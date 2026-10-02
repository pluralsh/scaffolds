# ssh-access

Entry point `SSHAccess`, package `internal/sshaccess`. Grants a Google user short-lived SSH
access to a Compute Engine instance through [OS Login](https://cloud.google.com/compute/docs/oslogin)
and an [IAP TCP tunnel](https://cloud.google.com/iap/docs/using-tcp-forwarding): no keys are
pushed to the instance and no port is opened to the internet.

Serve it with `just serve ssh-access`. Setup, calling conventions and the response format are
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
| `zone` | yes | Zone of the instance. |
| `instance` | yes | Instance name in `GOOGLE_CLOUD_PROJECT`. The recipes default to the fixture, `$P-ssh`. |
| `user` | yes | Email of the Google user. The recipes default to `$SSH_USER`, or your gcloud account. |
| `role` | no | `user` (default) logs in without sudo, `admin` with sudo. |
| `durationMinutes` | no | 1 to 1440, default 60, at most `MAX_DURATION_MINUTES` (terraform `ssh_access_max_minutes`, default 240). |
| `revoke` | no | `true` removes the access instead. |

```
just ssh-plan    [role] [minutes] [user] [instance]
just ssh-execute [role] [minutes] [user] [instance]
just ssh-revoke  [user] [instance]
```

## How it behaves

Guards, evaluated in this order:

| Guard | Passes when |
|---|---|
| `exists` | The instance exists. When it doesn't, this is the only guard in the response. |
| `os-login` | `enable-oslogin` is `TRUE` in the instance's metadata, or, if the instance doesn't set it, in the project's. Not checked when revoking. |
| `duration-allowed` | `durationMinutes` is at most `MAX_DURATION_MINUTES`. Not checked when revoking. |

Execute grants two roles to `user:<email>`, as bindings whose IAM condition
(`request.time < timestamp("...")`) ends the access on its own:

- `roles/compute.osLogin` (or `roles/compute.osAdminLogin`) on the instance;
- `roles/iap.tunnelResourceAccessor` on the instance's IAP tunnel resource.

Both conditions have the title `plural.sh ssh-access`, and only such bindings are changed.
Granting again keeps the later expiry, so it extends and never shortens; changing the role
replaces the binding. `revoke: true` removes both right away. Every execute also removes
expired bindings of the function on the instance (`result.expiredRemoved`). Policies are
written with the etag they were read with, so a concurrent change makes the call fail instead
of being overwritten. A grant writes the OS Login binding after the tunnel one, and a revoke
removes it first, so a failed write never leaves the user able to log in by mistake.

```
plan     -> planned; result.access {role, expiresAt} as execute would grant it, result.command
execute  -> done; changed true; same access, command to connect
execute  -> again with a shorter duration: done; changed false, expiry kept
revoke   -> done; no access, no command
```

`result.otherAccess` lists the OS Login and tunnel roles the user has on the instance
otherwise; the function neither grants nor revokes them, and roles on the project or above
aren't listed. If the instance runs as a service account (`result.instance.serviceAccount`),
OS Login also requires the user to have `roles/iam.serviceAccountUser` on it, which the
function doesn't grant. The connection also needs a firewall rule allowing IAP's range
`35.235.240.0/20` to port 22.

The custom role reads the instance and project metadata and reads and sets the IAM policies of
instances and IAP tunnel instances. Setting a policy allows granting any role on that resource;
only the function restricts it to these roles.

## Test fixtures

```bash
just ssh-fixtures       # $P-ssh with OS Login, $P-ssh-nologin without, firewall rule $P-ssh-iap for IAP
just ssh-policy         # the instance's and its tunnel's IAM policies
```

Test with a user other than yourself if you are a project owner: owners can connect anyway.

## Test matrix

See [Testing a function](../README.md#testing-a-function) for the conventions.

### A. Input validation, no GCP calls or credentials needed

| # | Request | Expected |
|---|---|---|
| A1 | `just ssh-plan root` | 400 `role "root" is not user or admin` |
| A2 | `just ssh-plan user 0` | 400 `durationMinutes 0 is not between 1 and 1440` |
| A3 | `just ssh-plan user 60 serviceAccount:x@p.iam.gserviceaccount.com` | 400 `user ... is not the email of a Google account` |

### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just ssh-plan user 60 $SSH_USER nope` | `refused`; only `exists` failed | - |
| B2 | `just ssh-plan` | `planned`; 3 guards pass, `os-login` from the instance metadata; `access.expiresAt` in an hour; `command` set; `instance.serviceAccount` the default compute account | `just ssh-policy`: no new bindings |
| B3 | `just ssh-plan user 60 $SSH_USER $P-ssh-nologin` | `refused`; `os-login` failed | - |
| B4 | `just ssh-plan user 300` | `refused`; `duration-allowed` failed (limit 240) | - |

### C. Execute

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just ssh-execute user 60 $SSH_USER $P-ssh-nologin` | `refused`; nothing granted | - |
| C2 | `just ssh-execute user 30` | `done`; `changed` true; `access` role `user`, expiry in 30 minutes | `just ssh-policy`: osLogin and tunnelResourceAccessor bindings with the condition |
| C3 | As the user: the `command` from C2 | Logs in without sudo (after `roles/iam.serviceAccountUser` on the instance's service account) | - |
| C4 | `just ssh-execute user 10` | `done`; `changed` false; expiry from C2 kept | - |
| C5 | `just ssh-execute admin 30` | `done`; role `admin`; osLogin binding replaced by osAdminLogin | `just ssh-policy` |
| C6 | `just ssh-revoke` | `done`; no `access`; `changed` true | `just ssh-policy`: both bindings gone; C3 is denied |
| C7 | `just ssh-execute user 1`, wait 2 minutes, `just ssh-revoke otheruser@example.com` | Second call: `expiredRemoved` 2 | Bindings gone |

### D. Permissions

Run as the `ssh-access` service account, see
[Running as the minimal service account](../README.md#running-as-the-minimal-service-account).

| # | Setup | Expected |
|---|---|---|
| D1 | Repeat B2, C2 and C6 | Same results. **Check that the IAP tunnel policy calls work with the instance name** in the resource name |
| D2 | `just role-remove ssh-access iap.tunnelInstances.setIamPolicy`, wait, restart, C2 | 502 `Provider`, 403 on the tunnel policy; `just ssh-policy`: no OS Login binding, as a grant writes it last |

Add the permissions back with `just role-add ssh-access <permission>`.

## Cleanup

```bash
just ssh-cleanup
just sa-delete ssh-access
```
