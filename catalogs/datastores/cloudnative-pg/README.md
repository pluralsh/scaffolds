# CloudNativePG

Deploys the [CloudNativePG](https://cloudnative-pg.io/) operator on all Plural workload clusters. CloudNativePG is a Kubernetes operator for running highly available PostgreSQL clusters natively on Kubernetes, with built-in support for replication, automated failover, backups (including continuous WAL archiving) and rolling upgrades.

## Prerequisites

- A Plural management cluster with workload clusters tagged `platform: k8s`
- No additional configuration is required — the operator installs with hardened defaults

## Usage

Once the operator is deployed, provision PostgreSQL clusters by applying `Cluster` resources to any workload cluster:

```yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: my-db
  namespace: my-app
spec:
  instances: 3
  storage:
    size: 10Gi
```

See the [CloudNativePG docs](https://cloudnative-pg.io/documentation/) for backup, monitoring and failover configuration.

## Notes

- The operator runs in the `cnpg-system` namespace (the upstream default).
- Adjust `helm.version` in `cloudnative-pg.yaml` to pin a different operator release.
