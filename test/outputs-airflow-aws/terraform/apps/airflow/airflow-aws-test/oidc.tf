resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "airflow" {
  name = "airflow-airflow-aws-test"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for airflow deployed to the airflow-aws-test cluster"
  redirect_uris = ["https://airflow.example.org/oauth-authorized/plural"]
}
