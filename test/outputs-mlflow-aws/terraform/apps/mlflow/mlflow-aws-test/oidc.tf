resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "mlflow" {
  name = "mlflow-mlflow-aws-test"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for mlflow deployed to the mlflow-aws-test cluster"
  redirect_uris = ["https://mlflow.example.org/oauth2/callback"]
}
