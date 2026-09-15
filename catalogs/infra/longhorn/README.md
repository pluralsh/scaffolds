# Longhorn

Deploys [Longhorn](https://longhorn.io/) across the fleet using the official Longhorn Helm chart.

Longhorn provides distributed block storage for Kubernetes, including dynamic persistent-volume provisioning, snapshots, backups, and a management UI. The chart installs Longhorn's control plane, CSI components, and supporting services in the `longhorn-system` namespace.

## Before installation

Longhorn nodes need the host prerequisites documented by the project, including `open-iscsi`/`iscsid` and the required kernel modules. Verify those prerequisites on every target node before enabling this GlobalService.

## After installation

Review Longhorn's default StorageClass and replica settings for the capacity and failure-domain layout of the target clusters. Configure a backup target separately if recurring off-cluster backups are required.

## Upstream

- Documentation: https://longhorn.io/docs/
- Helm repository: https://charts.longhorn.io
- Source: https://github.com/longhorn/longhorn
