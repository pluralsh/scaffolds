# Elasticsearch setup

This catalog installs ECK (Elasticsearch + Kibana), Logstash/Filebeat, and the
Plural **datastore operator**, which provisions ILM, index templates, and the
`plrl-logs-write` rollover alias via CRs in `services/datastores/`.

## Log rollover flow

1. **Datastore operator** applies CRs (sync waves 5–9):
   - `ElasticsearchCredentials` → in-cluster ES as user `plrl`
   - Legacy ILM `plrl-logs` (dated indices: warm 10d / delete 30d)
   - Rollover ILM `plrl-logs-rollover` (30gb shard thresholds, shrink @ 1d, delete 7d)
   - Templates for `plrl-logs-*` (legacy) and `plrl-logs-0*` (rollover + write alias)
   - Cleanup Job (removes a stray concrete `plrl-logs-write` index if Logstash created one first)
   - Bootstrap index `plrl-logs-000001` with alias `plrl-logs-write` (`is_write_index: true`)
2. **Logstash** writes to `plrl-logs-write` (`manage_template` / `ilm_enabled` false).
3. Elasticsearch rolls to `plrl-logs-000002`, … when ILM thresholds hit.

Query Console / DeploymentSettings with index pattern `plrl-logs-*` (covers rolled indices).

## Deploy order

On first install, prefer:

1. ECK operator → GeneratedSecrets → elastic-stack (ES healthy)
2. `datastore-operator` ServiceDeployment (ILM + bootstrap Ready)
3. Logstash GlobalService

If Logstash starts before the write alias exists, the cleanup Job + bootstrap CR
recover from a concrete `plrl-logs-write` index conflict.

## Tuning

Edit the CRs under `services/datastores/` (retention, shard sizes, rollover
thresholds). The bootstrap `ElasticsearchIndex` is create-only — existing
indices are not rewritten; new rolled indices pick up template updates.

## ECK Configuration

Elasticsearch's operator is full-featured; see
https://www.elastic.co/guide/en/cloud-on-k8s for node topology and other tuning
(for example
[node configuration](https://www.elastic.co/guide/en/cloud-on-k8s/current/k8s-node-configuration.html)).
