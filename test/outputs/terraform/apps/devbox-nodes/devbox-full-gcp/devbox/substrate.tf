locals {
  substrate = var.install_substrate ? 1 : 0
  project   = local.cluster_context.project_id
  # Service account ids are 6-30 characters, so the cluster part is capped.
  substrate_suffix = trim(substr(replace(lower(local.cluster_context.cluster_name), "/[^a-z0-9-]/", "-"), 0, 18), "-")
}

resource "google_storage_bucket" "substrate" {
  count                       = local.substrate
  project                     = local.project
  name                        = trim(substr("${local.project}-${local.substrate_suffix}-ate", 0, 63), "-")
  location                    = local.cluster_context.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
}

resource "google_service_account" "substrate_api_server" {
  count        = local.substrate
  project      = local.project
  account_id   = "ate-api-${local.substrate_suffix}"
  display_name = "Agent Substrate ate-api-server on ${local.cluster_context.cluster_name}"
}

# ate-api-server's credential exchange impersonates this account for every snapshot transfer, scoped to one
# actor, so atelet itself needs no access to the bucket.
resource "google_service_account" "substrate_transfer" {
  count        = local.substrate
  project      = local.project
  account_id   = "ate-xfer-${local.substrate_suffix}"
  display_name = "Agent Substrate snapshot transfers on ${local.cluster_context.cluster_name}"
}

resource "google_service_account_iam_member" "substrate_api_server_workload_identity" {
  count              = local.substrate
  service_account_id = google_service_account.substrate_api_server[0].name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${local.project}.svc.id.goog[ate-system/ate-api-server]"
}

# The ZeroFS controller and node plugins keep devbox homes under their own prefix of the bucket.
resource "google_service_account" "substrate_zerofs" {
  count        = local.substrate
  project      = local.project
  account_id   = "ate-zfs-${local.substrate_suffix}"
  display_name = "Agent Substrate ZeroFS devbox homes on ${local.cluster_context.cluster_name}"
}

resource "google_service_account_iam_member" "substrate_zerofs_workload_identity" {
  count              = local.substrate
  service_account_id = google_service_account.substrate_zerofs[0].name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${local.project}.svc.id.goog[ate-system/devbox-zerofs]"
}

resource "google_service_account_iam_member" "substrate_transfer_token_creator" {
  count              = local.substrate
  service_account_id = google_service_account.substrate_transfer[0].name
  role               = "roles/iam.serviceAccountTokenCreator"
  member             = "serviceAccount:${google_service_account.substrate_api_server[0].email}"
}

resource "google_storage_bucket_iam_member" "substrate" {
  for_each = var.install_substrate ? {
    "api-object-admin"       = { role = "roles/storage.objectAdmin", sa = "api" }
    "api-bucket-viewer"      = { role = "roles/storage.bucketViewer", sa = "api" }
    "transfer-object-admin"  = { role = "roles/storage.objectAdmin", sa = "transfer" }
    "transfer-bucket-viewer" = { role = "roles/storage.bucketViewer", sa = "transfer" }
    "zerofs-object-admin"    = { role = "roles/storage.objectAdmin", sa = "zerofs" }
    "zerofs-bucket-viewer"   = { role = "roles/storage.bucketViewer", sa = "zerofs" }
  } : {}

  bucket = google_storage_bucket.substrate[0].name
  role   = each.value.role
  member = "serviceAccount:${local.substrate_accounts[each.value.sa]}"
}

locals {
  substrate_accounts = var.install_substrate ? {
    api      = google_service_account.substrate_api_server[0].email
    transfer = google_service_account.substrate_transfer[0].email
    zerofs   = google_service_account.substrate_zerofs[0].email
  } : {}
}
