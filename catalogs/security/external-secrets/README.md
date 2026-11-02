# External Secrets Operator

[External Secrets Operator (ESO)](https://external-secrets.io) is a Kubernetes operator that integrates external secret management systems -- such as **AWS Secrets Manager**, **HashiCorp Vault**, **Azure Key Vault**, **Google Cloud Secret Manager**, and more -- into Kubernetes.

ESO reads information from external APIs and automatically injects the values into native Kubernetes `Secret` resources.

## Architecture Overview

External Secrets Operator extends the Kubernetes API with dedicated Custom Resource Definitions (CRDs) to manage the lifecycle of external secrets:

- **ExternalSecret**: Defines what data should be fetched from an external provider, how often to refresh, and how to map values into a target Kubernetes `Secret`.
- **SecretStore**: A namespaced resource that defines connection settings and authentication credentials for a specific external secret provider.
- **ClusterSecretStore**: A cluster-wide equivalent of `SecretStore` that can be referenced by `ExternalSecret` resources across all namespaces.
- **Webhook & CertController**: Validates ESO resources on creation/update and manages TLS certificates required for admission webhooks.

```
       +---------------------------------------------+
       | External Secret Providers                   |
       | (AWS Secrets Manager / Vault / Azure KV...) |
       +---------------------------------------------+
                              |
                              | (API sync via IAM / Workload Identity)
                              v
             +----------------------------------+
             |    External Secrets Operator     |
             |                                  |
             |  +---------------+  +---------+  |
             |  | SecretStore / |  | Webhook |  |
             |  | ClusterStore  |  +---------+  |
             |  +---------------+               |
             +----------------------------------+
                              |
                              | reconciles ExternalSecret
                              v
                +----------------------------+
                | Native Kubernetes Secret   |
                +----------------------------+
                              |
                              | mounted into
                              v
                +----------------------------+
                |    Workload / Pods         |
                +----------------------------+
```

### Key Workflow
1. The operator queries the specified external provider (e.g. AWS Secrets Manager, HashiCorp Vault, Azure Key Vault) using configured authentication.
2. Retrieved secret data is decoded and mapped according to the `ExternalSecret` specification.
3. A standard Kubernetes `Secret` is created or updated in the target namespace.
4. Applications consume the secret natively without requiring provider-specific SDKs.

## Configuration Parameters

The Plural catalog deploys the official Helm chart from `https://charts.external-secrets.io`. Key configuration parameters include:

| Parameter | Description | Default |
| --- | --- | --- |
| `installCRDs` | Automatically install and upgrade CustomResourceDefinitions with Helm | `true` |
| `crds.createClusterExternalSecret` | Create CRD for `ClusterExternalSecret` | `true` |
| `crds.createClusterSecretStore` | Create CRD for `ClusterSecretStore` | `true` |
| `replicaCount` | Number of operator controller replicas | `1` |
| `image.repository` | Controller container image repository | `ghcr.io/external-secrets/external-secrets` |
| `image.tag` | Controller container image tag | (Chart `appVersion`) |
| `webhook.create` | Deploy the validating webhook service | `true` |
| `certController.create` | Deploy certificate controller for managing webhook certificates | `true` |
| `leaderElect` | Enable leader election for controller instances | `false` |
| `resources` | CPU and memory resource requests/limits | `{}` |
| `serviceAccount.create` | Create service account for the operator | `true` |
| `serviceAccount.annotations` | Annotations for service account (e.g. IAM role ARN for IRSA/Workload Identity) | `{}` |

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds.
