# Airflow external PostgreSQL setup

This AWS catalog generates an Airflow deployment, S3 log storage, a managed PostgreSQL database and Plural OIDC configuration. The chart remains `airflow` 8.9.0 from the existing airflow-helm repository. The configured application image remains Airflow 2.8.4 with Python 3.9.

## Database routing

The chart consumes PostgreSQL configuration from the root `externalDatabase` values, not `web.externalDatabase`. The catalog disables its embedded PostgreSQL chart and supplies the stack's imported `postgres_host` and `postgres_password`, database `airflow`, user `airflow` and port 5432. JSON quoting keeps the imported host and password as YAML strings when Plural renders the service.

The existing PgBouncer deployment stays enabled. Airflow connects through PgBouncer, whose upstream now points to the managed database. Celery workers and the bundled Redis broker remain unchanged. In the rendered resources, expect no bundled PostgreSQL StatefulSet, a PgBouncer configuration referencing the imported host, and the expected database credentials in the chart's configuration Secret.

## Existing installations

This changes the database actually used by the application. If a previous deployment stored metadata in the bundled PostgreSQL instance, applying these values does not copy that data into the managed database. Before upgrading, back up the current database, plan and verify its migration to the external database, and preserve the keys needed to read existing encrypted Airflow connections. Do not assume an empty managed database contains your existing DAG state, users or connections, or discard the old database volume until recovery and migration are verified.

## Deployment prerequisites and limits

Complete the Terraform stack and configure the target cluster, cloud/network contexts, DNS, ingress and certificate issuer before relying on the deployment. The database endpoint must be reachable from the Airflow pods. Review the retained `externalDatabase.properties` and PgBouncer TLS settings against your database's encryption requirements; the local rendering test does not validate network access or TLS negotiation.

Key management is an existing separate prerequisite: the catalog's current `fernetKey` and `web.webserverSecretKey` values are not the `airflow.fernetKey` and `airflow.webserverSecretKey` settings consumed by this chart. The Terraform Fernet output is a 20-character password, rather than the URL-safe base64 encoding of a 32-byte Fernet key. Do not simply move that invalid value to the active setting. Configure valid, stable keys through the chart's supported settings and plan key migration for an existing database before production use. This database-routing change does not modify those keys or the OAuth configuration.

Local validation renders the pinned chart with synthetic stack outputs and checks database routing, credentials and resource selection. It does not provision AWS resources, migrate data or prove live Airflow startup/database connectivity. See the [pinned chart documentation](https://github.com/airflow-helm/charts/tree/airflow-8.9.0/charts/airflow) for its configuration contract.

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
