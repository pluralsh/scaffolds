# Stakater Reloader on Plural

This catalog installs the official Stakater Reloader Helm chart, pinned to
**2.2.17** (controller **v1.4.22**), in `reloader-system`. It is an internal
Kubernetes controller with no user interface, ingress, credentials to generate,
persistent storage, or additional CRDs required by this configuration.

## Before merging

The generated `GlobalService` follows this catalog's fleet installation pattern.
Review its cluster distribution in Plural before merging: a GlobalService without
distribution restrictions can install on all eligible clusters. Check each target
for an existing Reloader installation; keep one controller per watched scope to
avoid duplicate rollouts. The `apps` namespace and Plural deployment operator must
already be configured, and the target clusters must be able to pull the official
`ghcr.io/stakater/reloader:v1.4.22` image and reach the Kubernetes API.

Use a Kubernetes version supported by your Plural installation and verify this
pinned chart against that version in staging before production. This scaffold
does not introduce an independently tested Kubernetes compatibility matrix.

## Enable a workload

Installation alone does not opt workloads into reloads: `autoReloadAll` is false.
Add the following annotation to the **workload's metadata**, not the pod template,
for a Deployment, StatefulSet, or DaemonSet that should reload:

```yaml
metadata:
  annotations:
    reloader.stakater.com/auto: "true"
```

The workload must reference the ConfigMap or Secret through its environment or
volumes. A subsequent change to the referenced object causes Reloader to update
the pod template and let Kubernetes perform the rollout. Workloads without an
opt-in annotation stay unchanged. Named-resource and search/match annotations are
also supported by upstream; review those explicitly before adopting them.

The controller uses the `annotations` strategy instead of injecting environment
variables. It writes `reloader.stakater.com/last-reloaded-from` under pod-template
metadata. Review your GitOps diff/reconciliation settings for that generated
annotation; the strategy does not itself guarantee zero drift. Rollout behavior
still follows the workload's update strategy and availability settings.

Jobs, CronJobs, Argo Rollouts, OpenShift DeploymentConfigs, and CSI secret-provider
integration are disabled in this scaffold. Creation/deletion-trigger flags and
startup synchronization are also disabled. With the upstream annotations strategy,
a delete/recreate event can still cause a reload; do not treat those flags as a
guarantee that object recreation will be invisible to opted-in workloads.

## Permissions and resource use

The chart creates its service account and RBAC. `watchGlobally: true` grants
cluster-wide `get`, `list`, and `watch` access to Secrets and ConfigMaps, `get`,
`list`, `update`, and `patch` access to Deployments, DaemonSets, and StatefulSets,
and event creation/patching. Secret reads include object contents. An opt-in
annotation limits behavior, not RBAC access; only install this controller in
clusters where that access is appropriate. The disabled Job/CronJob integrations
avoid the chart's Job creation/deletion permissions. A separate Role in
`reloader-system` also permits creating and updating controller metadata ConfigMaps.

For a namespace-limited installation, review upstream `watchGlobally: false` and
`namespaces` settings and render the resulting Roles/RoleBindings before changing
the service. Workload or namespace selectors alone do not narrow a ClusterRole.

One non-root replica runs with a read-only root filesystem, no added capabilities,
and no privilege escalation. Its requests are 10m CPU / 128Mi memory and limits
are 150m CPU / 512Mi memory; adjust using actual cluster size and measurements.
Prometheus monitor CRDs, profiling, and outbound notification webhooks remain off.

## Verify after installation

```shell
kubectl -n reloader-system rollout status deployment/reloader --timeout=120s
kubectl -n reloader-system logs deployment/reloader --tail=100
```

In a disposable namespace, create one annotated Deployment and an otherwise
identical unannotated control, both referencing a test ConfigMap. Wait until both
are ready, record each pod-template annotation and pod UID, and update the
ConfigMap. Confirm only the annotated Deployment rolls out. Repeat with a test
Secret using non-sensitive data. Confirm an unrelated ConfigMap causes neither
workload to roll out. Observe rollout completion, not only controller logs, and
remove the disposable namespace after testing.

## Upgrade and remove

Update the pinned chart version in `bootstrap/apps/reloader/reloader-service.yaml`
through a reviewed PR. Review the upstream release notes, chart values and RBAC
changes; render for the target Kubernetes version and repeat the opt-in/control
checks before promotion. Roll back through Git to the previously validated pin
if necessary. Do not switch to an unbounded chart version.

To stop reloads for an application, remove its Reloader opt-in annotations through
Git and reconcile. To remove the controller, remove the generated GlobalService
through the normal Plural deletion workflow and verify its child services, Helm
resources, and RBAC are removed on the target clusters. Review resource-retention
settings before expecting automatic deletion. Application ConfigMaps and Secrets
are not owned by this controller; retain them. Existing application pods continue
running, but future configuration changes will no longer trigger Reloader rollouts.

## Upstream references

- [Reloader source and usage](https://github.com/stakater/Reloader/tree/v1.4.22)
- [Pinned Helm chart values](https://github.com/stakater/Reloader/blob/v1.4.22/deployments/kubernetes/chart/reloader/values.yaml)
- [Official chart repository](https://stakater.github.io/stakater-charts/)
- [Reloader documentation](https://docs.stakater.com/reloader/)
