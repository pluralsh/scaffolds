# Sealed Secrets

This is a baseline, prod-ready [Sealed Secrets](https://github.com/bitnami/sealed-secrets) installation using Plural. Sealed Secrets lets you encrypt Kubernetes `Secret` manifests into `SealedSecret` resources that are safe to commit to git — only the controller running in the cluster can decrypt them.

## What's included

- The official `sealed-secrets` Helm chart (v2.20.0, app v0.40.0) from `https://bitnami.github.io/sealed-secrets`, deployed as a Plural `GlobalService` into the `sealed-secrets` namespace on every workload cluster
- A hardened default config: non-root pod, read-only root filesystem, dropped capabilities, and modest resource requests/limits

## Prerequisites

- Kubernetes >= 1.16 on the workload cluster
- The controller manages its own sealing key pair on first start — no cloud credentials required. Back up the sealing keys (`kubectl get secret -n sealed-secrets -l sealedsecrets.bitnami.com/sealed-secrets-key`) if you need disaster recovery of sealed values.

## Usage

Seal a secret client-side with `kubeseal`, then commit the result:

```bash
kubectl create secret generic my-secret --from-literal=password=s3cret --dry-run=client -o yaml \
  | kubeseal --controller-namespace sealed-secrets --format yaml > my-sealed-secret.yaml
```

The `SealedSecret` unseals into a regular `Secret` in the same namespace once applied to the cluster.

## Upgrade notes

Bump `spec.template.helm.version` in `sealed-secrets.yaml` to roll a new chart version. Sealing keys persist across upgrades; rotating them invalidates previously sealed resources (see the [key renewal docs](https://github.com/bitnami/sealed-secrets#secret-rotation)).

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds.
