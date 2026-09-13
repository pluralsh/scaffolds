# Kured

This catalog installs [Kured](https://kured.dev/) (Kubernetes Reboot
Daemon), which performs safe, cordoned-and-drained node reboots when the
OS requests them, using the official `kured` Helm chart.

## What it deploys

- Kured chart `6.1.0` in the `kured` namespace.
- A daemonset watching `/var/run/reboot-required` (and sentinel files on
  other distros) that drains and reboots nodes one at a time.

## Prerequisites

- Kubernetes `1.23` or newer.
- Works with Ubuntu/Debian, Flatcar, Talos and other sentinel-file
  distributions; see the chart values for `--reboot-sentinel` paths.

## Using Kured

After the generated service is applied, inspect the daemonset:

```sh
kubectl -n kured get daemonset,pods
```

Reboot windows and drain timeouts are configured through the chart's
`configuration` values.
