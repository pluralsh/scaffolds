# Plural Jenkins

This catalog deploys [Jenkins](https://www.jenkins.io/) with the official
Jenkins Helm chart. It runs one persistent controller and schedules builds on
disposable Kubernetes agents, keeping build execution off the controller.

## What it deploys

- Jenkins chart `5.9.54` (Jenkins `2.568.2`) from `https://charts.jenkins.io`.
- One controller StatefulSet with zero controller executors.
- The chart's pinned Kubernetes, Pipeline, Git, and Configuration as Code plugins.
- A `ReadWriteOnce` persistent volume for `JENKINS_HOME`.
- An nginx ingress with cert-manager TLS at the configured hostname.
- Namespace-scoped RBAC for the controller to schedule agent pods.
- A dedicated default agent ServiceAccount with token automount disabled and a
  restricted Pod Security context.

The chart does not add a Secret-read rule to its controller Role. Always audit
external RoleBindings and ClusterRoleBindings, which can grant additional
permissions independently of this catalog.

## Prerequisites

The target cluster must have:

1. An nginx ingress controller using the `nginx` ingress class.
2. cert-manager and a `ClusterIssuer` named `plural`.
3. A default StorageClass that supports dynamically provisioned
   `ReadWriteOnce` volumes.
4. DNS for the configured hostname pointing to the ingress controller.
5. Outbound DNS and HTTPS access during the first start for container images,
   the Jenkins update center, and the pinned plugin downloads.

For an egress proxy, configure both `controller.initContainerEnv` (plugin setup)
and `controller.containerEnv` (the running controller). In an isolated network,
use an approved controller image with the required plugins already installed
and test it before deployment.

## Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `cluster` | Kubernetes cluster where Jenkins will run | Required |
| `hostname` | Lowercase DNS hostname for the HTTPS ingress | Required |
| `storageSize` | Persistent volume size for `JENKINS_HOME` | `20Gi` |

The ingress limits request bodies to `50m`. Increase that value deliberately in
`helm/jenkins/jenkins.yaml.liquid` if jobs accept larger file uploads.

## First login

The chart generates the initial password in a Kubernetes Secret; no password is
written to Git. Retrieve the username and password from the target cluster:

```bash
kubectl --context <kube-context> -n jenkins get secret jenkins \
  -o jsonpath='{.data.jenkins-admin-user}' | base64 --decode; echo

kubectl --context <kube-context> -n jenkins get secret jenkins \
  -o jsonpath='{.data.jenkins-admin-password}' | base64 --decode; echo
```

Sign in at `https://<hostname>/`, rotate or replace the bootstrap credential,
and configure your identity provider before granting broad access.

The `cluster` input is a Plural cluster handle; it is not necessarily the same
name as your local kubectl context.

### Credential lifecycle

By default, the chart owns Secret `jenkins` with keys `jenkins-admin-user` and
`jenkins-admin-password`. It uses Helm `lookup` to retain the password when the
renderer can query the live cluster. An offline or client-only render cannot see
that Secret and may show a newly generated desired password; do not treat that
render as a rotation procedure.

For a stable production lifecycle, create a Secret through your external secret
manager and point the generated values at it:

```yaml
controller:
  admin:
    createSecret: true
    existingSecret: jenkins-admin
```

The external Secret must exist before the controller starts and must contain the
same two keys. Keep `createSecret: true`: with `existingSecret` set, the chart
does not create the Secret but still mounts it and wires the bootstrap user into
JCasC. Never put the password directly in the values file.

To rotate it, update the external source, wait for `jenkins-admin` to contain the
new value, and restart `statefulset/jenkins` in a maintenance window. The JCasC
reload sidecar watches ConfigMaps, not Secret changes. Verify the new login and
invalidate the old credential before ending the maintenance window.

## Verification

Check the controller, persistent volume, and ingress:

```bash
kubectl --context <kube-context> -n jenkins rollout status statefulset/jenkins
kubectl --context <kube-context> -n jenkins get pods,pvc,ingress
```

Run a small Pipeline on a Kubernetes agent and confirm the executor appears as a
temporary pod in the `jenkins` namespace. The controller intentionally has zero
executors and should not run builds itself.

## Persistence and backups

`JENKINS_HOME` contains jobs, build history, plugin state, and the keys Jenkins
uses to protect stored credentials. Back up the complete volume consistently;
do not copy only selected files.

For a recoverable backup:

1. Put Jenkins into quiet-down mode, wait for running builds to finish, and
   stop the controller or use a storage-consistent snapshot mechanism.
2. Snapshot the entire PVC and record the chart/controller/plugin versions,
   generated values, and admin Secret source used with it.
3. Restore the snapshot into a new PVC, set `persistence.existingClaim` to that
   claim, and provide the coordinated admin Secret before starting Jenkins.
4. Test the restore in an isolated hostname/namespace. Verify jobs, credentials,
   build history, and a representative agent Pipeline before approving it.

Deleting the PVC can permanently remove Jenkins state, depending on the
StorageClass reclaim policy. Check that policy and backup status before deleting
the ServiceDeployment, release, or claim.

This catalog creates one controller backed by one `ReadWriteOnce` volume. It is
durable across pod replacement but is not active-active high availability.

## Upgrades and plugins

The ServiceDeployment pins the chart version, and the chart's direct plugin
versions are pinned. Plugin initialization runs once on the persistent volume
to avoid downloading unexpected versions during ordinary restarts. The marker
is `/var/jenkins_home/initialization-completed`.

`controller.sidecars.configAutoReload.enabled` is explicitly enabled, so JCasC
ConfigMap changes are loaded by the reload sidecar rather than depending on the
one-time plugin initialization path.

For an upgrade, review the Jenkins chart changelog and upgrade guide, update the
chart version deliberately, and decide whether the initialization marker must
be reset during a maintenance window so required plugin changes can be applied.
Back up `JENKINS_HOME` first and validate representative Pipelines after the
upgrade.

## Security boundaries

- TLS terminates at the ingress; traffic inside the cluster uses the chart's
  `ClusterIP` service.
- Anonymous access and Jenkins "remember me" are disabled.
- The default JNLP agent is non-privileged, receives a restricted Pod Security
  context, and uses `jenkins-agent` without an API token.
- Custom Jenkins pod templates can override these defaults. Enforce the
  restricted Pod Security Standard with namespace admission or an equivalent
  policy engine. Give jobs that genuinely need the Kubernetes API a separate,
  least-privileged ServiceAccount instead of enabling the default token.
- A generic NetworkPolicy is not enabled because Jenkins needs environment-
  specific access to DNS, source control, update sites, and build dependencies.
  Add a policy using your cluster's actual ingress labels and required egress
  destinations.
- SSO, automated backup scheduling, external secret management, and
  organization-specific agent templates remain operator responsibilities.

## Contributing

If there are features or documentation you would like to add, contribute at
https://github.com/pluralsh/scaffolds.
