resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "airbyte" {
  name = "airbyte-${var.cluster_name}"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for airbyte deployed to the ${var.cluster_name} cluster"
  redirect_uris = ["https://${var.hostname}/oauth2/callback"]
}
