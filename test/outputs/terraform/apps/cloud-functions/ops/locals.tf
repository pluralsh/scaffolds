locals {
  volume_arn   = "arn:${data.aws_partition.current.partition}:ec2:${var.region}:${data.aws_caller_identity.current.account_id}:volume/*"
  snapshot_arn = "arn:${data.aws_partition.current.partition}:ec2:${var.region}::snapshot/*"
  instance_arn = "arn:${data.aws_partition.current.partition}:ec2:${var.region}:${data.aws_caller_identity.current.account_id}:instance/*"
  eni_arn      = "arn:${data.aws_partition.current.partition}:ec2:${var.region}:${data.aws_caller_identity.current.account_id}:network-interface/*"
  # Tag the EBS CSI driver sets on volumes it creates for a PersistentVolumeClaim.
  pvc_tag_condition = { test = "Null", variable = "aws:ResourceTag/kubernetes.io/created-for/pvc/name", values = ["false"] }

  # Without allow_skip_snapshot, the `snapshot` input is removed from the tool schema and the
  # function rejects `snapshot: false` as well.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  # Every function that can be deployed. `statements` is the minimal IAM the function needs
  # on top of writing its own logs. Every function is registered as a workbench tool, and every
  # call of the tools of `destructive` functions, which change or delete resources, requires
  # human approval.
  catalog = {
    volume-delete = {
      binary      = "volume-delete-aws"
      description = "Deletes an unattached EBS volume that Kubernetes created for a PersistentVolume. Before calling it, confirm in the cluster that the PersistentVolume no longer exists and pass its name as pvName. Use action plan first; execute takes a snapshot and keeps the volume, and a later execute deletes it once the snapshot has completed."
      memory      = 128
      timeout     = 30
      destructive = true
      environment = { ALLOW_SKIP_SNAPSHOT = tostring(var.allow_skip_snapshot) }
      # Deleting and snapshotting are limited to volumes created for a PersistentVolumeClaim.
      statements = [
        { actions = ["ec2:DescribeVolumes", "ec2:DescribeSnapshots"], resources = ["*"], conditions = [] },
        { actions = ["ec2:DeleteVolume", "ec2:CreateSnapshot"], resources = [local.volume_arn], conditions = [local.pvc_tag_condition] },
        { actions = ["ec2:CreateSnapshot"], resources = [local.snapshot_arn], conditions = [] },
        {
          actions    = ["ec2:CreateTags"]
          resources  = [local.snapshot_arn]
          conditions = [{ test = "StringEquals", variable = "ec2:CreateAction", values = ["CreateSnapshot"] }]
        },
      ]
      schema = jsonencode(merge(local.volume_delete_schema, { properties = local.volume_delete_properties }))
    }
    vm-delete = {
      binary      = "vm-delete-aws"
      description = "Deletes a standalone EC2 instance with its root volume and network interfaces; data volumes are kept. Instances in an Auto Scaling group and EKS worker nodes are refused. Use action plan first to see what is deleted and kept, then execute; execute again if it reports that the instance is still updating."
      memory      = 128
      timeout     = 30
      destructive = true
      environment = {}
      # EKS and Auto Scaling membership are checked by the function; IAM cannot express those
      # refusals as tightly as the Azure resource-group scopes.
      statements = [
        { actions = ["ec2:DescribeInstances", "ec2:DescribeNetworkInterfaces"], resources = ["*"], conditions = [] },
        {
          actions    = ["ec2:TerminateInstances", "ec2:ModifyInstanceAttribute"]
          resources  = [local.instance_arn]
          conditions = []
        },
        {
          actions    = ["ec2:ModifyNetworkInterfaceAttribute"]
          resources  = [local.eni_arn]
          conditions = []
        },
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/vm-delete.json")))
    }
  }

  functions      = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown        = setsubtract(var.functions, keys(local.catalog))
  function_names = { for key, _ in local.functions : key => "${var.name}-${key}" }
  artifacts      = { for key, fn in local.functions : key => "${var.artifact_dir}/${var.artifact_version}/${fn.binary}.zip" }
}
