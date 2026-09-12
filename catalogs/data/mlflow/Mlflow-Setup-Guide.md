# MLflow setup

This catalog generates an AWS PostgreSQL stack, an S3 artifact bucket, and an MLflow service for {{ context.cluster }}. Configure the cloud credentials and network contexts referenced by the Terraform files, the `infra` GitRepository, and the `plural` SCM connection before provisioning.

## S3 artifact endpoint

For region `{{ context.region }}`, MLflow uses the regional service endpoint `https://s3.{{ context.region }}.amazonaws.com:443`. This is the service endpoint, not a bucket hostname. The bucket name is passed separately to the chart. Both MLflow and the chart's S3 connection-check init container use this endpoint.

This hostname format covers the catalog's existing standard AWS domain. AWS China endpoints require a different domain and are not configured by this template.

The deployment retains the Terraform-generated S3 access key and secret key, existing external PostgreSQL configuration, and OAuth proxy integration. Review the infrastructure plan, approve the stack, and confirm database connectivity and ingress prerequisites before relying on the deployment. The catalog's local contract test does not provision resources, connect to AWS, or verify a live artifact upload.

## Local validation

Run `plural pr contracts --file test/contracts-mlflow-aws.yaml --validate` from a clean checkout to compare generation against the committed AWS fixture. The Liquid values retain service-time `configuration` and `imports` expressions for Plural to resolve.

AWS regional endpoint reference: https://docs.aws.amazon.com/general/latest/gr/s3.html
