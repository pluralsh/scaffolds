# ssh-access: grants a user short-lived SSH access to an instance through OS Login and an IAP
# TCP tunnel. See functions/go/gcp/docs/ssh-access.md.

variable "ssh_access_max_minutes" {
  type        = number
  description = "Longest access ssh-access may grant, in minutes."
  default     = 240
  # The stack passes the installation field as is, which is null when it was left empty; null
  # then means the default.
  nullable = false

  validation {
    condition     = var.ssh_access_max_minutes >= 1 && var.ssh_access_max_minutes <= 1440
    error_message = "ssh_access_max_minutes must be between 1 and 1440 (a day)."
  }
}

locals {
  ssh_access = {
    entry_point = "SSHAccess"
    description = "Grants a Google user short-lived SSH access to a Compute Engine instance with OS Login enabled, through an Identity-Aware Proxy TCP tunnel, by granting an OS Login role on the instance and the IAP tunnel role, both expiring after durationMinutes, and returns the command to connect. revoke: true removes the access right away. Use action plan first to check the instance, then execute."
    memory      = "512Mi"
    timeout     = 30
    destructive = true
    apis        = ["compute.googleapis.com", "iap.googleapis.com"]
    environment = {
      MAX_DURATION_MINUTES = tostring(var.ssh_access_max_minutes)
    }
    # Setting an instance's or tunnel's IAM policy allows granting any role on it; only the
    # function restricts it to the OS Login and IAP tunnel roles. Project metadata tells whether
    # OS Login is enabled project-wide.
    permissions = [
      "compute.instances.get",
      "compute.instances.getIamPolicy",
      "compute.instances.setIamPolicy",
      "compute.projects.get",
      "iap.tunnelInstances.getIamPolicy",
      "iap.tunnelInstances.setIamPolicy",
    ]
    schema = jsonencode(jsondecode(file("${path.module}/schemas/ssh-access.json")))
  }
}
