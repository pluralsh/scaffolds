# n8n catalog setup

The catalog generates a Terraform stack for a managed PostgreSQL database and a ServiceDeployment for n8n on {{ context.cluster }} ({{ context.cloud }}). It uses the 8gears OCI chart `n8n` version `2.1.1`, whose default application version is `2.36.8`.

## Prerequisites

- Configure the `plural` SCM connection, `infra` GitRepository, target cluster, and the cloud credentials/network contexts referenced by the generated Terraform files.
- Provide a hostname and region. Azure also requires an existing resource group.
- Provide ingress-nginx, cert-manager with a `letsencrypt-prod` ClusterIssuer, and DNS pointing the hostname to the ingress.
- Approve and complete the generated Terraform stack before deploying the service; service templates read its `postgres_host`, `postgres_password`, and `encryption_key` outputs.
- Configure and validate database TLS for your cloud before relying on a live database connection. This catalog repair validates chart configuration delivery; it does not establish database connectivity.

## Database TLS prerequisite

The supplied GCP Terraform module requires encrypted connections (`ssl_mode = "ENCRYPTED_ONLY"`). The generated n8n values do not configure its PostgreSQL TLS settings, so that database connection requires further configuration before n8n can start successfully. Preserve the database encryption policy. Supply a trusted CA and an endpoint whose certificate identity can be verified, or integrate an authenticated Cloud SQL connector/proxy. The default private-IP endpoint and per-instance CA do not by themselves establish a verified TLS setup. Do not work around this by disabling certificate verification.

AWS RDS and Azure Flexible Server may also require TLS according to the instance policy. Configure the appropriate trusted CA and n8n PostgreSQL SSL settings for those endpoints and test connectivity. No cloud connection is exercised by the repository's generation contracts.

## Generated configuration

The chart consumes `main.config` as a ConfigMap and `main.secret` as a Kubernetes Secret. The PostgreSQL settings use n8n's `DB_POSTGRESDB_*` variables, with `DB_TYPE=postgresdb`. The imported database password and encryption key are supplied through the chart Secret; they are not placed in its ConfigMap. Keep the Terraform state, stack outputs, and rendered Secret access restricted to the deployment operators who need them.

The values template keeps `configuration` and `imports` expressions deferred until Plural renders the service. JSON quoting preserves their string values. The public editor and webhook URLs use the configured HTTPS hostname. The application Service listens on port 5678 and routes to the chart's n8n container port.

The deployment remains a single main process with an external PostgreSQL database. Queue workers, webhooks on separate processes, and Valkey are not enabled. Persistence and resource sizing should be reviewed for your workflow's local file and execution requirements.

## First deployment

Review the generated infrastructure plan, complete the database TLS prerequisite, and verify n8n can connect to PostgreSQL. Then open `https://{{ context.hostname }}` and complete n8n's initial owner-account setup. Preserve the generated encryption key when restoring or migrating the database so existing workflow credentials remain readable.

## Local validation

Run `plural pr contracts --file test/contracts-n8n-aws.yaml --validate`, and the corresponding `azure` and `gcp` contracts, from a clean checkout. These compare generated files with the committed fixtures; they do not provision cloud resources or validate cloud database connections.

Chart documentation: https://github.com/8gears/n8n-helm-chart

Cloud SQL TLS guidance: https://cloud.google.com/sql/docs/postgres/configure-ssl-instance
