resource "random_password" "oidc_cookie" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "plural_oidc_provider" "n8n" {
  name = "n8n-{{ context.cluster }}"
  auth_method = "BASIC"
  type = "PLURAL"
  description = "OIDC provider for n8n deployed to the {{ context.cluster }} cluster"
  redirect_uris = ["https://{{ context.hostname }}/oauth2/callback"]
}

resource "random_password" "basic_auth_password" {
  length      = 24
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

resource "random_password" "encryption_key" {
  length      = 32
  min_lower   = 1
  min_numeric = 1
  min_upper   = 1
  special     = false
}

