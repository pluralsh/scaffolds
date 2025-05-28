# Plural Airflow

This is a baseline, prod ready Airflow installation using Plural.  It includes a few main components:

* S3/GCS/etc to handle logs storage. 
* RDS/Google Cloud Sql, Azure Flexible Server to handle postgres. This gives you a robust RDBMS service to hold Airflow's core transactional data.
* Plural OIDC to handle authentication to Airflow. This uses Airflow's built in Fab security manager based oauth binding, configured directly in the helm chart.

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
