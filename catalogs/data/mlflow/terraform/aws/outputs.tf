output "access_key_id" {
  value = aws_iam_access_key.mlflow.id
}

output "secret_access_key" {
  value = aws_iam_access_key.mlflow.secret
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
  value = plural_oidc_provider.mlflow.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.mlflow.client_secret
  sensitive = true
}
