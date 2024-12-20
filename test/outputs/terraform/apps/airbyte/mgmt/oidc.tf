resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "airbyte" {
  name = "airbyte-mgmt"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for airbyte deployed to the mgmt cluster"
  redirect_uris = ["https://airbyte.plrl-dev-aws.onplural.sh/oauth2/callback"]
}