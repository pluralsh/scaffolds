resource "random_password" "admin" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "grafana" {
  name = "grafana-mgmt"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for grafana deployed to the mgmt cluster"
  redirect_uris = ["https://grafana.example.org/login/generic_oauth"]
}
