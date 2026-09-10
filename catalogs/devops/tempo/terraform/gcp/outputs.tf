output "bucket_name" {
  value = google_storage_bucket.tempo.name
}

output "service_account_email" {
  value = google_service_account.tempo.email
}

output "service_account_name" {
  value = local.service_account_name
}
