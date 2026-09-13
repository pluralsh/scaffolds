# Pushgateway verification

The standard contract check covers omitted settings, explicit Secret/storage
names with surrounding whitespace, empty settings, and optional AI context:

```sh
plural pr contracts --file test/contracts-pushgateway.yaml --validate
```

As with the other contracts, first generate snapshots without `--validate`
when intentionally updating them, then commit the reviewed output. The
validation command requires a clean Git worktree.

An optional native behavior check uses a separately downloaded official
Pushgateway binary and chart `3.8.0`. It requires Python 3.9+, PyYAML, and Helm:

```sh
python test/pushgateway-runtime.py --binary /path/to/pushgateway --helm /path/to/helm --chart /path/to/prometheus-pushgateway --output-dir /path/to/test-output
```

The script creates synthetic test credentials through the chart's own bcrypt
helper, binds only to loopback, and uses a fresh directory under `--output-dir`.
It verifies authentication, accepted and rejected metric updates, grouping
labels, periodic snapshots, restarts, and deletion of one group while another
group remains. It stops each process it starts and preserves logs and results.
It never connects to a production service or Kubernetes cluster.

For Windows v1.11.3, the persistence filename is relative to that isolated
working directory: the binary rejects backslash absolute paths in its snapshot
temporary-file operation. Kubernetes uses the separately rendered Linux
`/data/pushgateway.data` path. On Windows, restarts use process termination
after waiting for periodic snapshots; this does not verify SIGTERM handling.

Passing native checks does not validate Linux container permissions, PVC
provisioning, projected-Secret updates, cluster networking, Prometheus scraping,
or CNI network policy. Verify those on the target cluster before production use.
