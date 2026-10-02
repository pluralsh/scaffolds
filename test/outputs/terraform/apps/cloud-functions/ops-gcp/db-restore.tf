# db-restore: restores a Cloud SQL instance to a point in time, as a new instance. See
# functions/go/gcp/docs/db-restore.md.

locals {
  db_restore = {
    entry_point = "DBRestore"
    description = "Restores a Cloud SQL instance to a point in time as a new instance (a point-in-time clone with the source's settings); the source is never changed. Use action plan first to see the restore window, then execute with a new targetInstance name; the restore takes a while, and plan with the same parameters then reports the new instance's state and addresses."
    memory      = "512Mi"
    timeout     = 30
    destructive = true
    apis        = ["sqladmin.googleapis.com"]
    environment = {}
    # Every instance in the project. A clone can't change its source; only the function's
    # checks keep it from creating an instance under a name that already exists.
    permissions = [
      "cloudsql.instances.get",
      "cloudsql.instances.clone",
    ]
    schema = jsonencode(jsondecode(file("${path.module}/schemas/db-restore.json")))
  }
}
