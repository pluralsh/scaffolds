# vm-delete: deletes a standalone Compute Engine instance with its boot disk, keeping its data
# disks. See functions/go/gcp/docs/vm-delete.md.

locals {
  vm_delete = {
    entry_point = "VMDelete"
    description = "Deletes a standalone Compute Engine instance with its boot disk. Data disks are detached and kept, and local SSDs are lost. Instances created by a managed instance group, GKE nodes and instances with deletion protection are refused. Use action plan first to see what is deleted and kept, then execute. If it reports that the instance is still updating its disks, execute again."
    memory      = "512Mi"
    timeout     = 30
    destructive = true
    apis        = ["compute.googleapis.com"]
    environment = {}
    # Every instance in the project; IAM can't express the managed instance group and GKE
    # refusals, so the function checks them itself. Changing a disk's auto-delete flag needs
    # compute.disks.update.
    permissions = [
      "compute.instances.get",
      "compute.instances.delete",
      "compute.instances.setDiskAutoDelete",
      "compute.disks.update",
    ]
    schema = jsonencode(jsondecode(file("${path.module}/schemas/vm-delete.json")))
  }
}
