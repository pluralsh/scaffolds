# Stakater Reloader

This scaffold installs the official [Stakater Reloader](https://github.com/stakater/Reloader)
Helm chart through a Plural GlobalService. It pins chart **2.2.17** and application
**v1.4.22** in the `reloader-system` namespace.

Reloader rolls opted-in workloads when a Secret or ConfigMap they reference changes.
It does not modify the contents of those resources.

## Before installing

- Use a Kubernetes cluster supported by the pinned chart and an operational Plural
  management/deployment setup. Check the upstream release notes before upgrading.
- Check for an existing Reloader installation on each target cluster. Do not install
  overlapping controllers; this scaffold is for clusters without one.
- The GlobalService follows Plural's global-service targeting behavior. Review its
  target clusters before applying it.
- The controller watches across namespaces and needs cluster-wide access to read
  Secrets and ConfigMaps and patch supported workloads. Deploy it only where those
  permissions are appropriate.
- Configure the `plural` SCM connection and the `scaffolds` repository reference
  used by the catalog automation.

## Install and opt in a workload

Run the Reloader catalog automation, review the generated pull request, and merge
it through your normal Plural deployment process. It creates
`bootstrap/apps/reloader/reloader.yaml` and this documentation.

Automatic reload of every workload is disabled. Creation/deletion reload triggers
are also disabled. To opt in an existing Deployment that references a ConfigMap or
Secret through an environment variable or volume, add an annotation to the
Deployment's metadata (not its pod-template metadata):

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: example
  annotations:
    reloader.stakater.com/auto: "true"
# Keep the rest of your existing Deployment unchanged.
```

Reloader's `annotations` strategy updates pod-template annotations to trigger a
rollout. If your GitOps reconciler manages the same annotation fields, configure
the documented upstream reconciliation exclusions and verify that it does not
revert the trigger repeatedly. A reload restarts pods; applications must tolerate
rolling restarts and have suitable readiness checks.

## Verify in a disposable cluster

1. Confirm the Reloader Deployment becomes available in `reloader-system`.
2. Create a dummy ConfigMap and two disposable Deployments that reference it.
   Annotate only one Deployment with `reloader.stakater.com/auto: "true"`.
3. Wait for both Deployments to become ready, then update the ConfigMap.
4. Confirm the opted-in Deployment receives a new pod-template revision and
   completes its rollout. Confirm the unannotated control retains its revision.
5. Remove the sample resources after the check.

Use dummy data for this check. Rendering a chart does not establish that the
controller is running or that rollouts occur.

## Upgrade or remove

Review the upstream release notes and chart changes, then update the pinned
`spec.template.helm.version` through a reviewed Git change. Repeat the disposable
cluster check before upgrading other clusters.

Before removal, remove workload opt-in annotations and review the Plural
GlobalService deletion/pruning behavior. Delete the Reloader GlobalService through
your usual GitOps change, confirm the managed service/controller is removed from
each target cluster, and only then remove any remaining dedicated namespace.
Application ConfigMaps, Secrets, and Deployments are not part of this chart.

## Contributing

Contribute changes at https://github.com/pluralsh/scaffolds.
