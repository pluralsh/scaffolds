# DragonflyDB

DragonflyDB is an in-memory datastore compatible with Redis APIs. This catalog deploys a single internal DragonflyDB instance at `dragonfly.dragonfly.svc.cluster.local:6379`.

The service uses a ClusterIP and is not exposed publicly. Authentication is enabled with a generated password stored in the `dragonfly-auth` Kubernetes Secret under the `requirepass` key. Clients should read that Secret rather than placing the password in application configuration.

Persistence is enabled by default with a 1Gi `ReadWriteOnce` PVC. The chart leaves the StorageClass unset so Kubernetes can use the cluster default; without a suitable default or matching PV, the claim will remain pending. The StatefulSet's PVC retention behavior is not configured by this catalog, so operators should account for retained claims when scaling down or removing the service.

This is a single-replica deployment and is not highly available. Configure backups and HA separately when required.
