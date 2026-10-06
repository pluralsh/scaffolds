# lb-frontend-delete

Entry point `LBFrontendDelete`, package `internal/lbfrontenddelete`. Removes what a deleted
Kubernetes `LoadBalancer` Service left of its GKE load balancer, when the cloud provider didn't
clean it up, e.g. because the cluster was deleted first.

Serve it with `just serve lb-frontend-delete`. Setup, calling conventions and the response format
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
| `region` | yes | Region of the load balancer. The recipes use `$REGION`, which defaults to the region of `$ZONE`. |
| `forwardingRule` | yes | `a` and the first 31 hex digits of the Service UID without dashes: the name the GKE cloud provider gives every resource of the load balancer. The recipes default to the fixture's, `just --evaluate lb`. |
| `serviceName` | yes | `namespace/name` of the deleted Service, as in the resources' descriptions. The recipes default to `e2e/web`. |

```
just lb-plan    [forwardingRule] [serviceName]
just lb-execute [forwardingRule] [serviceName]
```

## How it behaves

For a `LoadBalancer` Service with the name `L` above, the GKE cloud provider creates, in the
Service's region (see `gce_loadbalancer_naming.go` in kubernetes/cloud-provider-gcp):

| Resource | Name | When |
|---|---|---|
| forwarding rule | `L` | always |
| target pool | `L` | external load balancers |
| regional backend service | `L` | internal load balancers |
| legacy HTTP health check / health check | `L` | `externalTrafficPolicy: Local`; otherwise the cluster's shared `k8s-<cluster>-node` check, which is never touched |
| firewall rules | `k8s-fw-L`, `k8s-L-http-hc` | always / with its own health check |
| static address | `L` | when the cloud provider reserved it |

Each resource's description is JSON naming the Service (`kubernetes.io/service-name`). The
function looks them up by name and only deletes those whose description names `serviceName`;
others are reported with step `keep`. Load balancers of GKE's L4 controller (`k8s2-...` names,
with GKE subsetting or backend service based external load balancers) aren't supported.

Guards, evaluated in this order:

| Guard | Passes when |
|---|---|
| `exists` | Something of the load balancer is left. When nothing is, this is the only guard in the response. |
| `service` | The forwarding rule's description names `serviceName`. Once the rule is deleted, it passes and only resources described as the Service's are deleted. |
| `service-load-balancer` | The forwarding rule forwards to a target pool or backend service. Only checked while the rule exists. |
| `no-healthy-backends` | No backend of the rule's target (or of the target named `L` once the rule is gone) is healthy. A healthy backend means the Service may still exist, or the nodes still answer the cluster's shared health check. |

Backends are asked about their health ten at a time, stopping at the first healthy one. A
backend Compute Engine no longer knows, such as a node of a deleted cluster still listed in a
target pool, counts as not healthy.

A resource can only be deleted once nothing uses it, so each execute deletes what is free and
reports every resource with its step:

```
plan     -> planned; forwarding rule `delete`, everything else `later`, remaining true
execute  -> deletes the forwarding rule (step `deleting`); remaining true
execute  -> deletes the target pool or backend service, firewall rules and address; remaining true
execute  -> deletes the health check; remaining false
execute  -> refused, `exists` failed: nothing left
```

If Compute Engine refuses a deletion because the resource is busy or still in use, e.g. while
the forwarding rule's deletion finishes, its step is `waiting` and a later execute retries.
A target pool or backend service another forwarding rule uses is kept, and so is an address in
use by anything else.

The custom role reads, deletes and checks the health of forwarding rules, target pools and
regional backend services, reads and deletes both kinds of health checks, firewall rules and
addresses, and lists forwarding rules (to find other users of a target). In a Shared VPC, the
firewall rules live in the host project, where the function doesn't see them.

## Test fixtures

The fixtures recreate the leftovers of an external load balancer of a Service with
`externalTrafficPolicy: Local` with gcloud, without a cluster. The target pool has no
instances, so no backend is healthy.

```bash
just lb-fixtures   # address, legacy health check, target pool, forwarding rule and two firewall rules named $(just --evaluate lb)
just lb-show       # what is left, with descriptions
```

Healthy backends, internal load balancers, shared targets and busy resources are covered by the
unit tests in `internal/lbfrontenddelete`. To see the health guard live, run B2 against the
load balancer of a real Service in a GKE cluster: its nodes are healthy, so it is refused.

## Test matrix

See [Testing a function](../README.md#testing-a-function) for the conventions.

### A. Input validation, no GCP calls or credentials needed

| # | Request | Expected |
|---|---|---|
| A1 | `just lb-plan a123` | 400 `forwardingRule "a123" is not the name of a Service load balancer` |
| A2 | `just lb-plan $(just --evaluate lb) web` | 400 `serviceName "web" is not namespace/name` |
| A3 | `REGION=europe-central2-a just lb-plan` | 400 `region "europe-central2-a" is not a Compute Engine region` |

### B. Plan never changes anything

| # | Request | Expected | Verify |
|---|---|---|---|
| B1 | `just lb-plan a00000000000000000000000000000000` | `refused`; only `exists` failed | - |
| B2 | `just lb-plan` | `planned`; all 4 guards pass; steps: forwardingRule `delete`; targetPool, both firewalls and address `later`; httpHealthCheck `later`; `remaining` true | `just lb-show`: everything still there |
| B3 | `just lb-plan $(just --evaluate lb) e2e/api` | `refused`; `service` failed, `The forwarding rule was created for Service e2e/web, not e2e/api.` | - |

### C. Execute

| # | Request | Expected | Verify |
|---|---|---|---|
| C1 | `just lb-execute $(just --evaluate lb) e2e/api` | `refused`; nothing deleted | `just lb-show`: unchanged |
| C2 | `just lb-execute` | `done`; forwardingRule `deleting` with an `operation`; `remaining` true | Forwarding rule gone after a few seconds; address `RESERVED` |
| C3 | Right after C2, before the rule is gone | `done`; resources the rule still uses `waiting`, or the rule `deleting` again | - |
| C4 | `just lb-execute` | `done`; targetPool, both firewalls and address `deleting`; httpHealthCheck `later` | Those gone after a few seconds |
| C5 | `just lb-execute` | `done`; httpHealthCheck `deleting`; `remaining` false | `just lb-show`: nothing left |
| C6 | `just lb-execute` | `refused`; `exists` failed | - |

### D. Permissions

Recreate the fixtures first (`just lb-cleanup && just lb-fixtures`), then run as the
`lb-frontend-delete` service account, see
[Running as the minimal service account](../README.md#running-as-the-minimal-service-account).

| # | Setup | Expected |
|---|---|---|
| D1 | Repeat B2 and C2 to C5 | Same results. **Check that reading backend health needs nothing beyond `.get`** (there is no `getHealth` permission) (B2 with an instance in the pool: `gcloud compute target-pools add-instances`) |
| D2 | `just role-remove lb-frontend-delete compute.firewalls.delete`, wait, restart, run C2 and C4 | C4: 502 `Provider`, 403 on the firewall; rerun after adding it back |

Add the permissions back with `just role-add lb-frontend-delete <permission>`.

## Cleanup

```bash
just lb-cleanup
just sa-delete lb-frontend-delete
```
