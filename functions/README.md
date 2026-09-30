# Plural operational functions

Implementations of common out-of-IaC cloud operations (orphaned volume, instance and
load balancer cleanup, node group resizing, short-lived SSH access, ...) packaged for each
cloud's FaaS and invokable as Plural workbench tools. AWS and Azure functions are Rust, GCP
functions are Go.

## Layout

```
crates/core    cloud-agnostic request/response envelope, guards and errors
crates/aws     AWS Lambda runtime glue and SDK helpers
crates/azure   Azure managed identity and a minimal Azure Resource Manager client
crates/http    HTTP runtime for Azure Functions custom handlers and Cloud Run
bins/<name>    one binary per operation and cloud, e.g. volume-delete-aws; Azure bins also
               hold azure/<function>/function.json for each function of their app
go/gcp         Go module with every GCP function, one Functions Framework entry point each
```

The Go module registers the entry points in `functions.go` and keeps everything else under
`internal/`:

```
internal/core          request/response envelope, guards, errors and the HTTP server
internal/compute       Compute Engine client (interface, SDK implementation, connector)
internal/volume        cloud-agnostic volume deletion rules
internal/volumedelete  the VolumeDelete function
```

`just check` needs `golangci-lint` v2, configured by `go/gcp/.golangci.yml`.

The GCP functions are not compiled here. The release holds their source as `functions-gcp.zip`,
which terraform uploads and Cloud Run functions builds (Cloud Functions 2nd gen, `go127`).

## Invocation contract

Functions receive the workbench tool input as a JSON object and answer with JSON:

```jsonc
// request
{ "action": "plan", "volumeId": "vol-0123", "pvName": "pvc-1234" }   // action: plan (default) | execute

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
just check   # cargo fmt, clippy and test, then gofmt, go vet, golangci-lint and go test in go/gcp
```

Azure handlers are tested against an in-process ARM mock (`functions_azure::mock`, behind
the `mock` feature, which only the bins' dev-dependencies enable): tests script responses by
method and path and assert on the requests the handler sent, including their bodies and
`If-Match`/`If-None-Match` headers.
