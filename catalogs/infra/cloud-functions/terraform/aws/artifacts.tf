# Temporary artifact path used until the zips are published to per-region S3 buckets
# (artifact_s3_bucket). The zip is downloaded on every run and written next to the module,
# which stores it in state and makes local_file show as created on every plan.

data "http" "artifact" {
  for_each = { for key, fn in local.functions : key => fn if local.download }

  url = "${var.artifact_base_url}/${local.artifact_key[each.key]}"

  lifecycle {
    postcondition {
      condition     = self.status_code == 200
      error_message = "Downloading ${self.url} failed with HTTP ${self.status_code}."
    }
  }
}

resource "local_file" "artifact" {
  for_each = data.http.artifact

  filename       = "${path.module}/.artifacts/${local.functions[each.key].binary}-${var.artifact_version}.zip"
  content_base64 = each.value.response_body_base64
}
