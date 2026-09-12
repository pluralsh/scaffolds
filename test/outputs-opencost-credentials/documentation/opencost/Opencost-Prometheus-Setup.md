# OpenCost Prometheus authentication

This catalog deploys OpenCost to the clusters selected by its GlobalService. Configure a Prometheus-compatible query URL reachable from those clusters. The catalog selects this external query URL instead of the chart's in-cluster Prometheus configuration. The existing chart version range and ServiceMonitor configuration are unchanged; install the monitoring CRDs required by the selected chart before deployment.

Choose one authentication mode:

- `existing_secret`: supply `secretName`. Before deployment, create that Secret in the `opencost` namespace of every target cluster. It must contain `username` and `password` keys. Use the same Secret name across the fleet; the credentials may differ by cluster. This mode keeps credential values out of the generated Helm values.
- `credentials`: supply both `username` and `password`. These values are written into the generated repository's Helm values, and the OpenCost chart creates a Kubernetes Secret from them. Only use this mode when that repository is an appropriate place to store those credentials. YAML quoting preserves characters such as apostrophes; it does not encrypt or conceal the values.

Only the inputs for the selected mode are required. The URL, Secret name and direct credentials are rendered as YAML strings without interpreting ordinary punctuation. Both modes retain the catalog's `username` and `password` Secret key names.

Before merging the generated PR, inspect its cluster distribution, query URL and authentication mode. After deployment, check OpenCost pod readiness and successful queries against the configured Prometheus backend. If authentication fails, verify the Secret's key names, namespace and permissions at the metrics backend without printing credential values. Local rendering tests validate the generated configuration; they do not prove a live backend accepts the credentials or returns the metrics OpenCost needs.

[OpenCost Helm chart](https://github.com/opencost/opencost-helm-chart/tree/main/charts/opencost)
