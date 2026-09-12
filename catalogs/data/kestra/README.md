# Plural Kestra

This catalog installs a production-oriented Kestra Open Source baseline on AWS, Azure, or Google Cloud. It provisions durable infrastructure and configures the official Kestra Helm chart with:

- PostgreSQL 14 for the Kestra queue and repository
- S3, Azure Blob Storage, or Google Cloud Storage for internal object storage
- workload identity for object-storage access; no static cloud credentials are written to Git
- native Kestra basic authentication initialized before the first public request
- a generated 256-bit encryption key for `SECRET` inputs and outputs
- NGINX ingress with TLS issued by cert-manager

The catalog defaults to a single Kestra standalone pod. The database and object storage are durable, but the application tier is not highly available. Split-component or multi-replica deployments should be designed and load-tested for the expected workload before production use.

## Prerequisites

- Plural service contexts for the target cluster and network
- NGINX Ingress Controller and cert-manager with a `letsencrypt-prod` ClusterIssuer
- AWS EKS Pod Identity, GKE Workload Identity, or Azure Workload Identity enabled on the target cluster
- an existing Azure Storage account in the cluster resource group with hierarchical namespace disabled when deploying to Azure
- network and DNS access from the target AKS cluster to the Azure Blob endpoint
- an existing GCP Private Service Access connection for `servicenetworking.googleapis.com`, with sufficient unallocated address space, managed by the platform network stack
- DNS for the selected hostname pointed at the ingress controller

## Configuration

| Parameter | Description |
| --- | --- |
| `cluster` | Target Plural cluster handle |
| `cloud` | Cloud provider: `aws`, `azure`, or `gcp` |
| `hostname` | DNS hostname for the Kestra UI and API |
| `adminEmail` | Valid email address for the initial Kestra administrator |
| `storageBucket` | Globally unique S3/GCS bucket name, or container name unique within the Azure Storage account |
| `region` | AWS region for the infrastructure stack; AWS only |
| `storageAccount` | Existing Azure Storage account; Azure only |
| `maxUploadSize` | Maximum request body accepted by NGINX; defaults to `100m` |
| `enableDockerRunner` | Enables Kestra's privileged rootless Docker-in-Docker sidecar; defaults to `false` |

## Security model

Terraform generates the database password, administrator password, and Kestra encryption key. They are marked sensitive in stack outputs and materialized in the cluster as the `kestra-secrets` Secret. The Git repository contains only runtime import expressions, not plaintext credentials.

The generated administrator password is available from the `admin_password` output of the `kestra-<cluster>` infrastructure stack. Treat Terraform state and Kubernetes Secret access as privileged because both contain runtime credentials.

Cloud storage permissions are scoped to the selected bucket or container:

- AWS: list/location permissions on the bucket and read/write/delete permissions on its objects
- GCP: `roles/storage.objectAdmin` on the bucket
- Azure: Storage Blob Data Contributor on the container

The catalog consumes the existing GCP Private Service Access connection but does not create or modify it. Keeping that shared connection in the platform network stack prevents application stacks from replacing one another's allocated peering ranges.

Docker-in-Docker is disabled by default because it requires a privileged sidecar. Enable it only when workflows need Docker-based script tasks and the cluster security policy permits privileged containers.

## Operations

The default pod request follows Kestra's minimum standalone sizing: 2 vCPU and 4 GiB of memory. Review application sizing, database sizing, backup retention, deletion protection, and object-storage lifecycle policy for the intended workload.

AWS and GCP enable database deletion protection by default. Azure relies on Infrastructure Stack approval because Flexible Server does not expose the same catalog-level deletion-protection control; review every Azure destroy plan carefully.

The Helm version tracks Kestra's `1.3.x` LTS line. Test upgrades in a non-production environment and follow Kestra's database migration guidance before promoting them.

## Contributing

Improvements are welcome at https://github.com/pluralsh/scaffolds.
