# Airflow external PostgreSQL setup

This AWS catalog generates an Airflow deployment, S3 log storage, a managed PostgreSQL database and Plural OIDC configuration. The chart remains `airflow` 8.9.0 from the existing airflow-helm repository. The configured application image remains Airflow 2.8.4 with Python 3.9.

## Database routing

The chart consumes PostgreSQL configuration from the root `externalDatabase` values, not `web.externalDatabase`. The catalog disables its embedded PostgreSQL chart and supplies the stack's imported `postgres_host` and `postgres_password`, database `airflow`, user `airflow` and port 5432. JSON quoting keeps the imported host and password as YAML strings when Plural renders the service.

The existing PgBouncer deployment stays enabled. Airflow connects through PgBouncer, whose upstream now points to the managed database. Celery workers and the bundled Redis broker remain unchanged. In the rendered resources, expect no bundled PostgreSQL StatefulSet, a PgBouncer configuration referencing the imported host, and the expected database credentials in the chart's configuration Secret.

## Existing installations

This changes the database actually used by the application. If a previous deployment stored metadata in the bundled PostgreSQL instance, applying these values does not copy that data into the managed database. Before upgrading, back up the current database, plan and verify its migration to the external database, and preserve the keys needed to read existing encrypted Airflow connections. Do not assume an empty managed database contains your existing DAG state, users or connections, or discard the old database volume until recovery and migration are verified.

## Deployment prerequisites and limits

Complete the Terraform stack and configure the target cluster, cloud/network contexts, DNS, ingress and certificate issuer before relying on the deployment. The database endpoint must be reachable from the Airflow pods. Review the retained `externalDatabase.properties` and PgBouncer TLS settings against your database's encryption requirements; the local rendering test does not validate network access or TLS negotiation.

## Persistent keys and migration

For a new installation, Terraform's `random_bytes.fernet` generates 32 random bytes once and persists them in Terraform state. The sensitive `fernet_key` output converts standard Base64 to URL-safe Base64, retaining its padding. It remains stable across normal applies while the resource state is preserved. The existing persistent Flask password supplies `flask_secret`. The chart consumes these outputs at `airflow.fernetKey` and `airflow.webserverSecretKey`, distributing them through its configuration Secret to the Airflow processes.

The Random provider requires version 3.6 or newer within major version 3. Protect and back up Terraform state and stack outputs; do not replace the random-key resource or discard its state as a way to redeploy the application.

Before updating an existing installation, securely identify and preserve its **effective deployed Fernet key or key list**, together with a database backup. The old top-level `fernetKey` and `web.webserverSecretKey` values were ignored by the chart. Its old 20-character Terraform Fernet output is not a valid Fernet key and is not evidence of which key encrypted the database. An unmodified chart used its own default, but operators may have overridden it; inspect the actual effective configuration rather than assuming the default. An intentionally empty Fernet setting also needs a separate migration plan; it must not be treated as an encrypted database using the unused Terraform output.

The sensitive Terraform input `fernet_key` accepts an existing valid key or a comma-separated rotation list. Supply it through a private Terraform variable input, not a committed plaintext file. Set it to the effective old key before the first upgrade if you need to retain that key. The default `null` selects the newly generated key and must not be applied blindly to existing encrypted data.

For planned rotation, follow the [Airflow 2.8.4 Fernet rotation procedure](https://airflow.apache.org/docs/apache-airflow/2.8.4/security/secrets/fernet.html):

1. Back up the database and old key, and pause writers and drain active work during a controlled rollout so processes do not write data with keys that other processes cannot read.
2. Prepend the new valid key to the entire existing key list (for a single old key, use `new_key,old_key`). Update all Airflow processes to use that list before resuming writers.
3. Run `airflow rotate-fernet-key` against the intended database, verify successful re-encryption and normal access to existing connection credentials and variables, then set the override to only `new_key` and roll out consistently.
4. Keep that final override unless you have deliberately synchronized the generated Terraform key with the active key. Clearing the override selects the separate generated key and can make existing data unreadable.

Changing the effective Flask secret invalidates existing web sessions. Plan for users to sign in again, and ensure all applicable Airflow processes receive the same stable value. The OAuth configuration itself is unchanged. Coordinate this key rollout with the database migration above; neither change migrates existing data automatically.

Local validation renders the pinned chart with synthetic stack outputs and checks database routing, credentials, key delivery and resource selection. It does not provision AWS resources, migrate data or prove live Airflow startup/database connectivity. See the [pinned chart documentation](https://github.com/airflow-helm/charts/tree/airflow-8.9.0/charts/airflow) for its configuration contract.

## Managing DAGs

After a lot of experience with airflow server mangement, we believe the best way to manage dag code is to bake them into your own docker image and use a containerized release workflow. There's a number of reasons for it, but the truth is airflow's monolithic comingled user and server code is brittle and its the best way to avoid serious issues due to dag code releases.

To do this, you simply need to extend the the `airflow.image.*` yaml block of the helm values file we generate for you at `helm/airflow/**`, eg:

```yaml
airflow:
  image:
    repository: apache/airflow # extend and customize this image with your own airflow build to import dags
    tag: 2.8.4-python3.9 # your customized tag
```


All you need to do is define your own dockerfile, eg:

```docker
FROM apache/airflow:2.8.4-python3.9


RUN pip install ... # your dependencies
COPY ... # your dags code
```

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds
