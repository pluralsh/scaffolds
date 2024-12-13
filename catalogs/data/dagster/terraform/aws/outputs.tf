output "iam_user" {
  value = aws_iam_user.dagster
}

output "access_key_id" {
  value = aws_iam_access_key.airbyte.id
}

output "secret_access_key" {
  value = aws_iam_access_key.airbyte.secret
  sensitive = true
}

output "postgres_host" {
  value = try(module.db.db_instance_address, "")
}

output "postgres_password" {
  value = random_password.password.result
  sensitive = true
}

output "oidc_cookie_secret" {
  value = random_password.oidc_cookie.result
  sensitive = true
}

output "oidc_client_id" {
  value = plural_oidc_provider.airbyte.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.airbyte.client_secret
  sensitive = true
}
