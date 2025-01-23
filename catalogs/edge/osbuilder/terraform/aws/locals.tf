locals {
  workspace = yamldecode(data.local_sensitive_file.workspace.content)
  subdomain = lookup(local.workspace.spec, "network", "subdomain")
}

data "local_sensitive_file" "workspace" {
  filename = "${path.module}/../../workspace.yaml"
}