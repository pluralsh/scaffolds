# Flow Setup Documentation

Plural Flows provide a clean interface for developers to manage their applications on Kubernetes alongside the surrounding infrastructure that they likely involve.  A key benefit is the integration between deployment, observability, and AI troubleshooting Plural can provide as a central broker between all these operational layers.  The self-service flow setup here involves three main GitOps components:

```
bootstrap/
  flows/
    {flow}.yaml - entrypoint service to setup the flow

services/flow - a set of liquid files that template the subcomponents of the flow (dev/prod services, pipeline, observer to listen to img changes)

helm/flows/{type}.values.yaml.liquid - helm values files which can be used to configure the charts involved
```

We use a number of helm charts meant to model common infrastructure patterns, you are free to fork them or simply use our upstream versions for simplicity.

## Helm Charts 

Stateless - defined here: https://github.com/pluralsh/bootstrap/tree/main/charts/stateless.  This chart is simply modeling a stateless webserver type of deployment, and includes values toggles to wire in ingress, custom images, etc.  You are also free to add arbitrary secrets to this service which are added as env vars to the underlying application.

## Best Practices

Generally speaking further customization of our defaults are going to come in a few flavors, and you should try to implement them as generally as possible:

1. I want to customize the release stages for the flow - in this case modifying files within `services/flows` will propagate those new defaults to all flows listening to that folder via GitOps
2. I want to customize the default values used by a chart - just make a pr to the helm/flows/{values-file} file and it'll propage globally.

For more flow-specific configurations, a simple way to achieve that would be writing a default override values file in `helm/flows` and then allowing it to be configured via the yaml templating in `services/flows/{dev | prod}.yaml`.  Eg something like the following could work:

{% raw %}
```yaml
apiVersion: deployments.plural.sh/v1alpha1
kind: ServiceDeployment
metadata:
  name: {{ configuration.name }}-dev
spec:
  namespace: {{ configuration.name }}
  name: {{ configuration.name }}
  flowRef:
    name: {{ configuration.name }}
  repositoryRef:
    kind: GitRepository
    name: infra
    namespace: infra
  git:
    ref: main
    folder: helm/flows
  helm:
    version: x.x.x
    url: https://pluralsh.github.io/bootstrap
    chart: stateless
    valuesFiles:
    {% if configuration.custom == "true" %}
    - {{ configuration.name }}.values.yaml.liquid
    {% endif %}
    - stateless.values.yaml
  configuration:
    name: {{ configuration.name }}
    domain: {{ configuration.devDomain }}
    registry: {{ configuration.registry }}
    tag: {{ configuration.devTag }}
  clusterRef:
    kind: Cluster
    name: {{ configuration.devCluster }}
    namespace: infra
```
{% endraw %}

If you need to change the entire topology of the flow on a flow-specific basis, the recommended approach would be to create an entirely separate folder, eg: `services/flows/custom` and point that specific flow to it and all its yaml contents.