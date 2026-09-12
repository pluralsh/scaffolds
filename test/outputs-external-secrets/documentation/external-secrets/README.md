# External Secrets Operator

This is a baseline, prod-ready [External Secrets Operator](https://external-secrets.io) (ESO) installation using Plural. ESO syncs secrets from external secret managers (AWS Secrets Manager, GCP Secret Manager, Azure Key Vault, Vault, and many more) into native Kubernetes `Secret` resources.

## What's included

- The official `external-secrets` Helm chart (v2.10.0) from `https://charts.external-secrets.io`, deployed as a Plural `GlobalService` into the `external-secrets` namespace on every workload cluster
- CRDs installed via the chart (`installCRDs: true`), including `SecretStore`, `ClusterSecretStore`, `ExternalSecret` and `PushSecret`
- The ESO validating webhook and cert controller enabled
- Conservative default resource requests/limits on the controller

## Prerequisites

- Kubernetes >= 1.20 on the workload cluster
- The operator itself needs no cloud credentials; authentication is configured per-`SecretStore`/`ClusterSecretStore` (IRSA on AWS, Workload Identity on GCP/Azure, or service-account tokens)

## Next steps after install

Create a `SecretStore` (or `ClusterSecretStore`) pointing at your provider and then `ExternalSecret` resources that map remote keys to Kubernetes secrets, e.g.:

```yaml
apiVersion: external-secrets.io/v1
kind: SecretStore
metadata:
  name: aws-secrets-manager
  namespace: external-secrets
spec:
  provider:
    aws:
      service: SecretsManager
      region: us-east-1
      auth:
        jwt:
          serviceAccountRef:
            name: external-secrets
```

See the [official provider docs](https://external-secrets.io/latest/provider/aws-secrets-manager/) for provider-specific auth (IRSA, Pod Identity, etc.).

## Upgrade notes

Bump `spec.template.helm.version` in `external-secrets.yaml` to roll a new chart version. ESO CRDs are owned by the chart; `installCRDs: true` keeps them in sync on upgrade. The webhook requires port 443 admission access from the Kubernetes API server — on private clusters ensure the control plane can reach the `external-secrets-webhook` service.

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds.
