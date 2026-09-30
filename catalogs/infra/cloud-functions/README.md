# Plural cloud functions

Operational cloud functions for work that usually happens outside of IaC, such as cleaning
up resources orphaned by Kubernetes, packaged for each cloud's FaaS and registered as Plural
workbench tools. The function sources live in `functions/` of
[pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).

## Components

- `bootstrap/apps/cloud-functions/<name>/stack.yaml`: InfrastructureStack that deploys the functions.
- `terraform/apps/cloud-functions/<name>`: terraform for the selected cloud.
  - AWS: one arm64 Lambda per function with a least-privilege execution role and a
    CloudWatch log group, an IAM policy allowing only to invoke the functions, and one
    `LAMBDA` workbench tool per function.
  - Azure: one Flex Consumption function app (custom handler) per function, each with its
    own system-assigned managed identity, in the resource group of the mgmt cluster; a role
    allowing only to resolve the function keys, and one `AZURE_FUNCTION` workbench tool per
    function.
  - GCP: one Cloud Run service per function running as its own service account, in the
    project of the mgmt cluster, reachable only by identities granted `roles/run.invoker`,
    and one `CLOUD_RUN` workbench tool per function. The services run the function binary
    on Cloud Run's OS-only base image without a container build, from a private bucket.
    This Cloud Run feature is in Preview.

An init container of the stack run downloads the function packages from the GitHub release
and verifies them against its `SHA256SUMS`, so the mgmt cluster needs to reach `github.com`
and Docker Hub (`curlimages/curl`) while the stack runs. The packages are deployed into
Lambda, Azure and the GCP bucket, so the functions don't depend on the release afterwards.

## Available functions

| Function | Clouds | Changes resources | Description |
|---|---|---|---|
| `echo` | AWS, Azure, GCP | No | Echoes its input and the identity the function runs as. Verifies the setup. |

## After the stack is applied

1. Grant the cloud connection permission to invoke the functions:
   - AWS: attach the `invoke_policy_arn` output to the IAM principal of the cloud connection.
   - Azure: assign the `invoke_role_definition_id` output to the service principal of the
     cloud connection on the resource group.
   - GCP: set the invoker service account when installing, or grant the cloud connection
     service account `roles/run.invoker` on the services.
2. Add the tools from the `workbench_tool_ids` output to a workbench.

## Invocation contract

Every function takes `action`: `plan` evaluates safety checks without changing anything and
`execute` performs the operation if all checks pass. Calls are synchronous, so functions
submit the change and report the resource state without waiting for it to complete.

## Customizations

Terraform variables not set by the stack, such as `log_retention_days` (AWS),
`resource_group_name` and `instance_memory_in_mb` (Azure), `max_instance_count` (GCP) or
`tags`, can be added to `variables` in the stack.

## Contributing

See [pluralsh/scaffolds](https://github.com/pluralsh/scaffolds).
