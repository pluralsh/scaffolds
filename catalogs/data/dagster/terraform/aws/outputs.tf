output "bucket_name" {
  value = aws_s3_bucket.dagster.bucket
}

output "service_account_name" {
  value = local.service_account_name
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
  value = plural_oidc_provider.dagster.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.dagster.client_secret
  sensitive = true
}
