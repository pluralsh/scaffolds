# Argo Workflows

This catalog installs [Argo Workflows](https://argoproj.github.io/workflows/), the container-native workflow engine for Kubernetes, using the official Argo Helm chart.

## What it deploys

- The Argo Workflows controller and Argo Server (API and UI) in the `argo-workflows` namespace.
- Chart version `2.0.6`, which packages Argo Workflows `v4.1.3`.
- Two replicas each of the workflow controller and Argo Server for availability.
- The Workflow, CronWorkflow, WorkflowTemplate, and related CRDs.
- Controller metrics enabled on the built-in endpoint; a Prometheus `ServiceMonitor` is left disabled until a monitoring stack is selected.
- The Argo Server exposed on an internal `ClusterIP` service with `client` auth mode; no ingress, TLS termination, or external route is configured by default.
- Hardened default security contexts from the upstream chart: non-root execution, dropped Linux capabilities, and no privilege escalation.

The catalog installs the engine only. Teams submit `Workflow`, `WorkflowTemplate`, and `CronWorkflow` resources separately and choose artifact repositories and storage backends explicitly.

## Prerequisites

- The cluster must be Kubernetes `1.28` or newer (chart `2.0.x` supports Argo Workflows `v4.x`, which requires at least Kubernetes `1.27`).
- The cluster must be able to pull the official chart and the `quay.io/argoproj/workflow-controller`, `quay.io/argoproj/argocli`, and `quay.io/argoproj/argoexec` images.
- Workflow pods that consume artifacts require a configured artifact repository (for example S3, GCS, or MinIO); this catalog does not provision one.
- The Argo Server listens unencrypted on its ClusterIP. Terminate TLS at an ingress or reverse proxy before exposing it outside the cluster.

## Using the controller

After the generated service is applied, inspect the deployment and CRDs:

```sh
kubectl -n argo-workflows get deploy,pods,svc
kubectl get crd workflows.argoproj.io workflowtemplates.argoproj.io cronworkflows.argoproj.io
```

Port-forward the Argo Server to reach the UI locally:

```sh
kubectl -n argo-workflows port-forward svc/argo-workflows-server 2746:2746
```

Then submit a workflow:

```sh
argo submit -n argo-workflows --watch https://raw.githubusercontent.com/argoproj/argo-workflows/main/examples/hello-world.yaml
```
