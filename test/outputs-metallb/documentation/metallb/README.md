# MetalLB

This catalog installs [MetalLB](https://metallb.io/), the load balancer
implementation for bare-metal and on-premise Kubernetes clusters, using the
official `metallb` Helm chart.

## What it deploys

- MetalLB chart `0.16.1` in the `metallb-system` namespace.
- The controller deployment and speaker daemonset that announce
  `LoadBalancer` service IPs.

## Prerequisites

- Kubernetes `1.23` or newer, on infrastructure without a native cloud
  load balancer.
- One or more free IP ranges on the cluster network.

## Using MetalLB

MetalLB needs an address pool before it can assign IPs:

```sh
kubectl apply -f - <<EOF
apiVersion: metallb.io/v1beta1
kind: IPAddressPool
metadata:
  name: default-pool
  namespace: metallb-system
spec:
  addresses: [192.168.1.240-192.168.1.250]
EOF
```

See the [MetalLB configuration docs](https://metallb.io/configuration/)
for L2/BGP advertisement options.
