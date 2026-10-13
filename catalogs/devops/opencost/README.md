# Plural OpenCost

Deploys OpenCost to every cluster and a `KubecostExtractor` (`provider: OpenCost`) that ships cost data back to Plural.

## Migrating from Kubecost

1. Run this catalog entry.
2. Delete `bootstrap/apps/kubecost/`, `helm/kubecost/`, `services/kubecost/` and `documentation/kubecost/` from your infra repo. Deleting the `kubecost` and `kubecost-extractor` GlobalServices removes Kubecost from your clusters.
