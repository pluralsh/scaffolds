# Plural n8n

This is a baseline, production-ready n8n installation using Plural. It includes a few main components:

* **PostgreSQL Database** - RDS (AWS), Cloud SQL (GCP), or Azure Flexible Server to hold n8n's workflow data, credentials, and execution history.
* **Plural OIDC** - Authentication to n8n is handled via oauth-proxy as middleware, since n8n's native auth is limited in enterprise SSO scenarios.
* **Encryption Key** - A randomly generated encryption key for securing sensitive workflow data and credentials stored in the database.

## Architecture

n8n is deployed as a Kubernetes service with the following components:

1. **n8n Application** - The main workflow automation engine
2. **PostgreSQL** - External managed database for persistence
3. **OAuth2 Proxy** - Sidecar for OIDC-based authentication
4. **Ingress** - NGINX ingress with TLS termination via cert-manager

## Configuration Options

When deploying n8n through the Plural catalog, you'll configure:

| Parameter | Description |
|-----------|-------------|
| `cluster` | The Kubernetes cluster to deploy n8n to |
| `cloud` | Cloud provider (aws, gcp, or azure) |
| `hostname` | DNS name for accessing n8n (e.g., `n8n.example.com`) |
| `region` | Cloud region for provisioning resources |
| `resourceGroup` | (Azure only) Azure resource group |

## Post-Deployment

### Accessing n8n

Once deployed, n8n will be available at the hostname you specified. Authentication is handled via Plural's OIDC provider - users will authenticate through your Plural organization.

### Basic Auth

Basic Auth for your n8n instance is configured by default alongside the OpenID Connect setup brokered by Plural. The basic auth user is hardfixed to `n8n` and the randomly-generated password can be found in the stack outputs of your generated n8n stack, which should be named something like `n8n-{cluster-name}`.

### Encryption Key

The encryption key used by n8n to secure credentials and sensitive data is auto-generated and stored in the Terraform state. You can retrieve it from the stack outputs if needed for debugging or migration purposes.

## Scaling Considerations

The default deployment is suitable for small to medium workloads. For high-volume workflow execution, consider:

1. Enabling n8n's queue mode with Redis for horizontal scaling
2. Increasing database instance size
3. Configuring resource limits in the Helm values

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds

