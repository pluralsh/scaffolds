# trust-manager on workload-west

This installs Jetstack trust-manager chart and application `v0.25.0` into the
`trust-manager` namespace on the selected cluster. It installs the Bundle CRD,
controller and admission webhook. It creates no Bundles and exposes no public UI.

## Before installation

Install and verify cert-manager, including its CRDs, webhook and CA injector, on
this same cluster. The chart uses cert-manager to issue and renew its webhook
certificate. If certificate request approval is restricted (for example with
approver-policy), configure approval for this webhook certificate before rollout;
the chart's approver-policy integration is not enabled by this catalog.

Create the trust source namespace `organization-trust` before installing.
Use a dedicated namespace holding public CA certificates and restrict who can
modify its contents. The upstream chart grants Secret read access in this namespace,
plus cluster-wide ConfigMap read/write permissions for bundle distribution. Secret
targets remain disabled, so it does not grant cluster-wide Secret read/write access.
Bundle management and namespace labels should be controlled by administrators.
Install only one trust-manager instance per cluster because Bundles are cluster scoped.

## Opt in to a bundle

After the controller and webhook are ready, create a ConfigMap named `organization-ca`
in `organization-trust` with a `ca.crt` key containing your approved public
PEM CA certificate(s). Never copy a private key into that ConfigMap. Then explicitly
label only the destination namespaces you want to receive this trust bundle:

```sh
kubectl label namespace YOUR_APPLICATION_NAMESPACE trust.example.com/organization-ca=enabled
```

Apply the following Bundle separately through your normal GitOps workflow:

```yaml
apiVersion: trust.cert-manager.io/v1alpha1
kind: Bundle
metadata:
  name: organization-trust
spec:
  sources:
  - configMap:
      name: organization-ca
      key: ca.crt
  target:
    configMap:
      key: ca-bundle.crt
    namespaceSelector:
      matchLabels:
        trust.example.com/organization-ca: enabled
```

This writes `organization-trust` ConfigMaps only in matching namespaces. Keep the
selector explicit: an empty selector can distribute to every namespace. Check the
Bundle's `Synced` condition and the destination ConfigMap before configuring an
application to mount and use `ca-bundle.crt`. Updating the source refreshes the bundle;
applications may need a reload or restart to consume the new trust store. Plan CA
rotation with an overlap of old and new trust roots before removing an old root.

The chart keeps the Bundle CRD on uninstall. Review existing Bundles and consuming
applications before removal or upgrades. Generation and chart rendering do not prove
that your certificates or application TLS connections work in your cluster.

See the upstream [installation guide](https://cert-manager.io/docs/trust/trust-manager/installation/),
[usage guide](https://cert-manager.io/docs/trust/trust-manager/), and
[pinned chart values](https://github.com/cert-manager/trust-manager/blob/v0.25.0/deploy/charts/trust-manager/values.yaml).
