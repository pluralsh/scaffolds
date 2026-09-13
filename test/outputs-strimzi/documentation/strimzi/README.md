# Strimzi (Apache Kafka)

This catalog installs the [Strimzi](https://strimzi.io/) operator, the
standard way to run Apache Kafka on Kubernetes, using the official
`strimzi-kafka-operator` Helm chart.

## What it deploys

- Strimzi operator chart `1.2.0` in the `strimzi` namespace.
- The cluster operator deployment, which manages `Kafka`, `KafkaTopic`,
  `KafkaUser` and related custom resources in its own namespace
  (`watchAnyNamespace: false`).

## Prerequisites

- Kubernetes `1.25` or newer.

## Using Strimzi

After the generated service is applied, create a Kafka cluster:

```sh
kubectl -n strimzi apply -f - <<EOF
apiVersion: kafka.strimzi.io/v1beta2
kind: KafkaNodePool
metadata:
  name: controller
  labels: {strimzi.io/cluster: kafka}
spec:
  replicas: 1
  roles: [controller, broker]
  storage:
    type: jbod
    volumes:
    - id: 0
      type: persistent-claim
      size: 20Gi
EOF
```

See the [Strimzi quickstart](https://strimzi.io/quickstarts/) for a full
Kafka + node pool example.
