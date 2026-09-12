# Plural Gatus

This catalog deploys [Gatus](https://github.com/TwiN/gatus), a service-health
dashboard, using the upstream `gatus` Helm chart from
`https://twin.github.io/helm-charts`.

## What it deploys

- A single Gatus replica in the `gatus` namespace, with chart version `1.5.0`.
- A 1Gi ReadWriteOnce volume for the SQLite health-check history at
  `/data/gatus.db`.
- A `Recreate` deployment strategy so an update cannot overlap two application
  replicas writing the same database.
- An internal ClusterIP service on port 80, with no Ingress or HTTPRoute.
- One HTTP endpoint checked every 60 seconds, healthy when it returns HTTP 200.
- Gatus metrics enabled at `/metrics`; a Prometheus Operator installation is
  not required to deploy this catalog.

The chart's non-root container and read-only root filesystem defaults are
preserved. Privilege escalation is disabled, Linux capabilities are dropped,
and the container uses the runtime's default seccomp profile. Gatus does not
need a mounted Kubernetes service-account token for these HTTP checks.

## Inputs

| Input | Meaning |
| --- | --- |
| `cluster` | Target cluster, which must have a suitable StorageClass and permission to fetch the chart and image. |
| `endpointName` | Display name of the first endpoint. |
| `endpointUrl` | Full HTTP(S) health URL reachable from that cluster, expected to return HTTP 200. |
| `storageClass` | Optional StorageClass for the volume; omit or leave empty to use the cluster default. |

The endpoint URL is written to the generated Git repository. Do not put
credentials or bearer tokens in it. For authenticated checks, configure Gatus
environment-variable references and an existing Secret separately; do not
commit credentials as Helm values.

## Using the generated deployment

The automation writes a ServiceDeployment under
`bootstrap/apps/gatus/<cluster>/` and its Helm values under
`helm/gatus/<cluster>/`. The service uses the existing `infra` GitRepository
in the `infra` namespace, consistent with the other catalog applications.

With your Kubernetes context set to the selected target cluster, use:

```sh
kubectl -n gatus port-forward service/gatus 8080:80
```

Open `http://localhost:8080` for the dashboard. Endpoints are checked by Gatus
inside the cluster, so a laptop-only URL will not be reachable by the pod.

To monitor more services or use other Gatus conditions, edit the generated
`config.endpoints` list. See the upstream [configuration guide](https://github.com/TwiN/gatus#configuration).
The chart annotates configuration checksums so changes restart the workload.

Before exposing the dashboard outside the cluster, explicitly configure TLS
and appropriate access controls for the health information it contains.
This catalog leaves that decision to the cluster owner.

## Storage and availability

The selected StorageClass must provision a writable ReadWriteOnce volume
compatible with the chart's default UID/GID 65534. A pending PVC keeps Gatus
from starting; inspect the PVC and the storage provider's events if needed.

This is a single-replica SQLite setup. Updates have a brief interruption,
and it is not a highly available monitoring service. Increasing replicas
without changing the storage design is unsupported. Back up the volume or
move to a suitable external database if longer-term requirements demand it.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
