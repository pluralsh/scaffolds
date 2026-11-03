# Argo Events

Deploys [Argo Events](https://argoproj.github.io/argo-events/) across the fleet using the official Argo Helm chart.

Argo Events provides an event-driven automation framework for Kubernetes. It installs the controller and custom resource definitions used to run `EventSource`, `EventBus`, and `Sensor` resources in application namespaces.

## After installation

Create an `EventBus`, one or more `EventSource` resources, and a `Sensor` that maps incoming events to Kubernetes-native triggers. Keep credentials for event sources in Kubernetes secrets and reference them from the corresponding Argo Events resources rather than embedding credentials in the catalog deployment.

## Upstream

- Documentation: https://argoproj.github.io/argo-events/
- Helm repository: https://argoproj.github.io/argo-helm
- Source: https://github.com/argoproj/argo-events
