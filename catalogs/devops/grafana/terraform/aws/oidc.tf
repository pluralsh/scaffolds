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
  name = "grafana-{{ context.cluster }}"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for grafana deployed to the {{ context.cluster }} cluster"
  redirect_uris = ["https://{{ context.hostname }}/login/generic_oauth"]
}
