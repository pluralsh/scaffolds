# Velero

This catalog installs [Velero](https://velero.io/), the standard tool for
backing up and restoring Kubernetes cluster resources and persistent
volumes, using the official `velero` Helm chart.

## What it deploys

- Velero chart `12.1.0` in the `velero` namespace.
- The Velero server deployment with credentials and restic/node-agent
  integration disabled, so it starts cleanly before cloud credentials are
  configured.
- No `BackupStorageLocation` or `VolumeSnapshotLocation` is preconfigured —
  add one for your cloud provider after installation.

## Prerequisites

- Kubernetes `1.23` or newer.
- To actually store backups you need an object storage bucket (S3, GCS or
  Azure Blob) plus cloud credentials; configure them through the chart's
  `credentials` and `configuration.backupStorageLocation` values.

## Using Velero

After the generated service is applied, inspect the components:

```sh
kubectl -n velero get pods,svc
```

Then create a backup storage location and run your first backup following
the [Velero documentation](https://velero.io/docs/).
