# Plural n8n

This is a baseline, production-ready n8n installation using Plural. It includes the following main components:

* **PostgreSQL Database** - RDS (AWS), Cloud SQL (GCP), or Azure Flexible Server to hold n8n's workflow data, credentials, and execution history.
* **n8n Built-in Authentication** - n8n's native user management system for authentication and access control.
* **Encryption Key** - A randomly generated encryption key for securing sensitive workflow data and credentials stored in the database.

## Architecture

n8n is deployed as a Kubernetes service with the following components:

1. **n8n Application** - The main workflow automation engine with built-in user management
2. **PostgreSQL** - External managed database for persistence
3. **Ingress** - NGINX ingress with TLS termination via cert-manager

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

### Initial Setup

On first access to n8n, you'll be prompted to create an owner account. This account will have full administrative access to the n8n instance.

### User Management

n8n's built-in user management allows you to:
- Create and manage user accounts
- Assign roles and permissions
- Configure LDAP/SAML for enterprise SSO (optional)

### Encryption Key

The encryption key used by n8n to secure credentials and sensitive data is auto-generated and stored in the Terraform state. You can retrieve it from the stack outputs if needed for debugging or migration purposes.

## Scaling Considerations

The default deployment is suitable for small to medium workloads. For high-volume workflow execution, consider:

1. Enabling n8n's queue mode with Redis for horizontal scaling
2. Increasing database instance size
3. Configuring resource limits in the Helm values

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
