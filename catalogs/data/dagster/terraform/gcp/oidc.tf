resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "dagster" {
  name = "${local.cluster_name}-dagster"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for Dagster deployed to the ${local.cluster_name} cluster"
  redirect_uris = ["https://{{ context.hostname }}/oauth2/callback"]
}
