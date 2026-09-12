# Kyverno

This catalog installs the official Kyverno policy engine through the Kyverno Helm chart.

## What it deploys

The generated Plural service installs the Kyverno chart in the kyverno namespace. The chart manages Kyverno's CRDs and controllers using the upstream Kyverno Helm repository.

No Policy or ClusterPolicy objects are included. Installing this catalog does not enforce policies. Teams can add policy resources separately and choose Audit or Enforce explicitly for each policy.

## Prerequisites

Before creating the generated pull request, make sure that:

- the target Kubernetes cluster is supported by the selected Kyverno chart;
- the target repository can create the kyverno namespace, cluster-scoped CRDs, admission webhooks, service accounts, and RBAC resources;
- the cluster can reach the upstream Kyverno Helm repository;
- the Plural SCM connection named plural is available for the PR automation.

Kyverno is a cluster-level policy engine. Review its CRD and webhook permissions with the cluster administrator before applying the generated change.

## Validation

The repository contract test renders this automation into the documentation and bootstrap output paths and checks that the generated manifests stay in sync with the catalog source. It does not claim a live Kubernetes-cluster installation or policy-enforcement validation.

## Contributing

If there are features or documentation you would like to add, please contribute back at https://github.com/pluralsh/scaffolds.
