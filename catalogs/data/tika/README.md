# Plural Apache Tika

Deploys [Apache Tika 4.0.0](https://tika.apache.org/4.0.0/) as an internal HTTP
service for extracting document text, metadata, and detected media types.
The official minimal image is pinned by its multi-platform digest.

## Access prerequisites

Tika does not provide user authentication. Use this catalog only with a CNI
that enforces Kubernetes ingress and egress NetworkPolicy. A ClusterIP alone
does not isolate it from other workloads.

| Input | Meaning |
| --- | --- |
| `cluster` | Linux cluster with enforced NetworkPolicy and capacity for the pod's resource limits. |
| `clientNamespace` | Existing namespace of trusted callers. Leading/trailing whitespace is stripped. |

Only pods labeled `tika-client: "true"` in the specified client namespace are
allowed ingress on TCP 9998. The namespace and pod selectors are in the same
peer, so **both** must match. The policy allows no application egress; caller
pods must separately have egress permission to the service. Kubernetes node
traffic and authorized port-forward access are not controlled by these peer
selectors. Do not use them as a replacement for protecting cluster credentials.

Do not expose this service publicly or connect mutually untrusted clients
without an appropriate authenticated gateway and transport controls. The
catalog creates no Ingress, HTTPRoute, LoadBalancer, or additional access role.

## What is deployed

- A single Deployment, ClusterIP Service and dedicated ServiceAccount in `tika`.
- A generated ConfigMap and a NetworkPolicy selecting only the Tika pods.
- No persistent volume; uploaded/spooled files and caches use a 512Mi temporary
  volume, which is discarded when a pod is replaced.
- UID/GID 35002 from the official image, a read-only root filesystem, dropped
  capabilities, no privilege escalation, default seccomp and no service-account
  token mount. Java temporary files and home/cache directories use `/tmp`.
- A 256MiB main JVM heap and one isolated parser JVM with a 768MiB heap.
  The pod requests 500m CPU/512Mi memory and is limited to 4 CPUs/2Gi memory.
  Heap sizes do not include all native JVM memory, buffers and process overhead.

The automation writes the service under `bootstrap/apps/tika/<cluster>/` and
the Kustomize input under `kubernetes/tika/<cluster>/`, using the existing
`infra` GitRepository in namespace `infra`. Kustomize hashes the ConfigMap
contents and updates the Deployment reference, so a configuration change
changes the pod template and triggers a rollout.

## Extraction profile

- Maximum request body: 10MiB. Requests above the cap are rejected with 413.
- Inputs above 1MiB are spooled to temporary storage rather than sent inline
  to the parser process. The internal IPC payload limit is 8MiB.
- Per-document total timeout: 60 seconds; stalled progress timeout: 15 seconds.
- Extracted-content write limit: 1,000,000 characters; embedded-document limit:
  100. A bounded or partial parse must not be treated as a complete extraction.
- One parsing slot. Waiting more than one second for a free parser can return
  429; callers should honor `Retry-After` and retry with bounded backoff.
- Enabled core endpoint groups: `tika`, `rmeta`, `meta`, `detect`, and `version`.
  Pipes/batch fetching, unpacking, status and per-request parser configuration
  are not enabled. No external fetcher, emitter or custom plugin is configured.

Use the [4.x server guide](https://tika.apache.org/docs/4.0.x/migration-to-4x/migrating-tika-server-4x.html)
when changing the profile. Several 3.x configuration fields and endpoints no
longer exist, and unknown configuration keys fail startup. Change heap, IPC,
input, output and temporary-storage limits together when increasing capacity.

The minimal image does not bundle external OCR/GDAL tools. Text extraction
from a PDF with a text layer is different from OCR of a scanned page. This
profile does not add remote AI, translation, OCR or scientific-parser services.

## Use and verify

From an allowed client pod, the endpoint is
`http://tika.tika.svc.cluster.local:9998`. For a local check with an authorized
Kubernetes context:

```sh
kubectl -n tika port-forward service/tika 9998:9998
```

In another terminal, upload a non-sensitive fixture that you own:

```sh
curl --fail http://localhost:9998/version
curl --fail -T fixture.pdf http://localhost:9998/tika/text
curl --fail -T fixture.pdf http://localhost:9998/tika/json/text
curl --fail -T fixture.pdf http://localhost:9998/detect
```

Tika 4 selects extraction format by URL path. The bare `/tika` endpoint
returns Markdown; an `Accept` header no longer selects text versus XML there.
Use `/tika/text` for body text and `/tika/json/text` for text with metadata.

Verify the NetworkPolicy on your actual CNI: an appropriately labeled pod in
the selected namespace should connect; an unlabeled pod there and a labeled
pod in another namespace should not. Verify the absence of outbound network
access too. Rendering a NetworkPolicy does not prove your cluster enforces it.

## Health and failure handling

Startup, readiness and liveness probes use `/version`. They verify the HTTP
process, not a successful parse of every document format. Include a small
periodic extraction check in your monitoring if parser readiness matters.

Inspect HTTP status and returned metadata, including `tk:exception:*` fields.
A response may contain partial text after an extraction error or a limit is
reached. 429 represents temporary contention; 503 can indicate a crashed or
timed-out parser. Repeatedly sending the same failing bytes is not a recovery
strategy. Tika may return exception detail, file names and document fragments;
filter results appropriately before forwarding them to a less trusted system.

Process isolation and resource limits improve fault containment but are not a
complete sandbox for malicious documents. Keep the pinned image current and
review the upstream security advisories. This deployment has no durable job
queue: callers must manage retries and retain their source documents.

## Local regression check

The repository includes `test/tika-runtime.py`. Supply Java 17+ and a verified,
fully extracted Tika 4.0.0 server ZIP (including `lib/` and `plugins/`):

```sh
python test/tika-runtime.py --java /path/to/java \
  --tika-home /path/to/tika-server --output-dir /path/to/test-results
```

The script starts an isolated loopback server, preserves this profile's limits
and gates, creates its own TXT/HTML/DOCX/PDF fixtures, and terminates only its
test process tree. It changes the bind address, port and local Java/temp paths.
On Windows it uses relative ASCII classpaths to avoid Java launcher argument-file
encoding loss when the workspace path contains non-ASCII characters.

The check covers Unicode extraction, metadata and format detection, disabled
endpoints, per-request configuration rejection, temporary spooling, output and
upload limits, and a successful parse after rejected inputs. It records the
result and server log in a new directory on each run. This does not verify a
live Kubernetes deployment, CNI enforcement, container volume permissions,
OCR, timeout/OOM recovery, or graceful SIGTERM handling on Windows.

## Contributing

Contributions are welcome at https://github.com/pluralsh/scaffolds.
