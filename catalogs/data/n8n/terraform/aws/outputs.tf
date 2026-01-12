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
  value = plural_oidc_provider.n8n.client_id
  sensitive = true
}

output "oidc_client_secret" {
  value = plural_oidc_provider.n8n.client_secret
  sensitive = true
}

output "basic_auth_username" {
  value = "n8n"
}

output "basic_auth_password" {
  value = bcrypt(random_password.basic_auth_password.result)
  sensitive = true
}

output "encryption_key" {
  value = random_password.encryption_key.result
  sensitive = true
}

