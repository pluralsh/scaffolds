# The zip is downloaded on every run and written next to the module for zip_deploy_file,
# which stores it in state and makes local_file show as created on every plan.
# TODO(PROD-5251): Flex Consumption only deploys local packages (zip_deploy_file / the publish
# API), so there is no server-side copy from the release. Replace the download with a file
# fetched before plan (e.g. a stack hook once the harness image ships curl).

data "http" "artifact" {
  for_each = local.functions

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
