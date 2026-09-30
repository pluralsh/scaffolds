# Plural operational functions

Rust implementations of common out-of-IaC cloud operations (orphaned volume, instance and
load balancer cleanup, node group resizing, short-lived SSH access, ...) packaged for each
cloud's FaaS and invokable as Plural workbench tools.

## Layout

```
crates/core    cloud-agnostic request/response envelope, guards and errors
crates/aws     AWS Lambda runtime glue and SDK helpers
bins/<name>    one binary per operation and cloud, e.g. volume-delete-aws
```

## Invocation contract

Functions receive the workbench tool input as a JSON object and answer with JSON:

```jsonc
// request
{ "action": "plan", "volumeId": "vol-0123" }   // action: plan (default) | execute

// response
{
  "action": "plan",
  "outcome": "planned",                          // planned | refused | done
  "guards": [{ "name": "unattached", "passed": true, "detail": "no attachments" }],
  "result": { ... }
}
```

`plan` is a dry run: it evaluates the guards and describes the change without making it.
`execute` evaluates the guards again and only acts when all of them pass. A guard failure is
a `refused` outcome, not an error, so the caller can see why nothing happened.

Invocations are synchronous and there is no progress reporting: workbenches wait up to 5
minutes on AWS and 30 seconds on GCP and Azure. `execute` therefore submits the change and
returns the resource state it observed (e.g. `deleting`) without waiting for completion.
Completion can be confirmed with a read-only cloud query.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```
