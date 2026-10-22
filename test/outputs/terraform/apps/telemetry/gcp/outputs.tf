output "bucket_name" {
  value = google_storage_bucket.telemetry.name
}

output "service_account_email" {
  value = google_service_account.telemetry.email
}
