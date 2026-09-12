# Grafana datasource provisioning

This catalog can configure the Plural Cloud Prometheus datasource, a Tempo datasource, both, or neither. The existing Grafana chart range (`8.6.x`), database, OAuth and deployment settings are unchanged by datasource provisioning.

- Enable `pluralCloud` to provision `prometheus` as the default datasource. The ServiceDeployment attaches the `plrl/cloud/observability` service context. That context must provide `vmetrics.query_url`, `vmetrics.user` and `vmetrics.password` when Plural renders the service for deployment.
- Supply `tempoUrl` to provision `tempo`. Use a query endpoint reachable from the Grafana deployment, such as `http://tempo.tempo.svc.cluster.local:3200` when the services share a cluster. Leave the optional input unset if Tempo is not needed.
- With neither option selected, the catalog does not add datasource provisioning configuration.

The chart's `datasources` value is a map of provisioning filenames. The generated `datasources.yaml` entry contains `apiVersion: 1` and its datasource list. Prometheus authentication uses `basicAuthUser` and `secureJsonData.basicAuthPassword`, as required by Grafana's datasource provisioning format.

The Plural Cloud context expressions intentionally remain in the generated `.liquid` values file. They resolve at service deployment time, after the service context is available. The resolved URL and credentials are added to the existing `grafana-env` Secret and exposed through environment variables. JSON string encoding preserves their YAML values. The datasource references those variables using Grafana's single-dollar syntax (`$GF_PLURAL_PROMETHEUS_URL`, `$GF_PLURAL_PROMETHEUS_USER` and `$GF_PLURAL_PROMETHEUS_PASSWORD`), so dollar signs inside the resolved values are not expanded again. The Tempo URL is rendered from the catalog input during PR generation.

Inspect the generated provisioning values before deployment. Once Grafana is running, verify the expected datasource names and test connectivity from Grafana. Local scaffold and Helm rendering checks do not prove that a live metrics or tracing backend is reachable or accepts the configured credentials. The provisioning ConfigMap contains environment-variable references rather than the Plural Cloud credentials. Kubernetes Secrets still require appropriate access controls; do not print their resolved values.

- [Grafana Helm chart](https://github.com/grafana/helm-charts/tree/grafana-8.6.4/charts/grafana)
- [Grafana datasource provisioning](https://grafana.com/docs/grafana/latest/administration/provisioning/#data-sources)
