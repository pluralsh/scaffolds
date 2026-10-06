locals {
  volume_arn    = "arn:${data.aws_partition.current.partition}:ec2:${var.region}:${data.aws_caller_identity.current.account_id}:volume/*"
  snapshot_arn  = "arn:${data.aws_partition.current.partition}:ec2:${var.region}::snapshot/*"
  instance_arn  = "arn:${data.aws_partition.current.partition}:ec2:${var.region}:${data.aws_caller_identity.current.account_id}:instance/*"
  eni_arn       = "arn:${data.aws_partition.current.partition}:ec2:${var.region}:${data.aws_caller_identity.current.account_id}:network-interface/*"
  nodegroup_arn = "arn:${data.aws_partition.current.partition}:eks:${var.region}:${data.aws_caller_identity.current.account_id}:nodegroup/*/*/*"
  asg_arn       = "arn:${data.aws_partition.current.partition}:autoscaling:${var.region}:${data.aws_caller_identity.current.account_id}:autoScalingGroup:*:autoScalingGroupName/*"
  lb_arn        = "arn:${data.aws_partition.current.partition}:elasticloadbalancing:${var.region}:${data.aws_caller_identity.current.account_id}:loadbalancer/*/*/*"
  # ARN of a target group, which has no type in its path.
  target_group_arn = "arn:${data.aws_partition.current.partition}:elasticloadbalancing:${var.region}:${data.aws_caller_identity.current.account_id}:targetgroup/*/*"
  db_instance_arn  = "arn:${data.aws_partition.current.partition}:rds:${var.region}:${data.aws_caller_identity.current.account_id}:db:*"
  iam_policy_arn   = "arn:${data.aws_partition.current.partition}:iam::${data.aws_caller_identity.current.account_id}:policy/plural.sh/ssh-access/*"
  iam_user_arn     = "arn:${data.aws_partition.current.partition}:iam::${data.aws_caller_identity.current.account_id}:user/*"
  iam_role_arn     = "arn:${data.aws_partition.current.partition}:iam::${data.aws_caller_identity.current.account_id}:role/*"
  # Tag the EBS CSI driver sets on volumes it creates for a PersistentVolumeClaim.
  pvc_tag_condition = { test = "Null", variable = "aws:ResourceTag/kubernetes.io/created-for/pvc/name", values = ["false"] }
  # Tags that mark load balancers and target groups created for Kubernetes: by the AWS Load
  # Balancer Controller, which tags the cluster, and by the Kubernetes cloud provider, which
  # tags the Service. Separate statements allow either.
  controller_tag_condition   = { test = "Null", variable = "aws:ResourceTag/elbv2.k8s.aws/cluster", values = ["false"] }
  service_name_tag_condition = { test = "Null", variable = "aws:ResourceTag/kubernetes.io/service-name", values = ["false"] }

  # Without allow_skip_snapshot, the `snapshot` input is dropped from the tool schema and the
  # function also rejects `snapshot: false`.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  # Deployable functions. `statements` is the minimal IAM each needs besides writing its logs.
  # All are registered as workbench tools; calls to `destructive` ones (which change or delete
  # resources) need human approval.
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
      description = "Deletes a standalone EC2 instance with its root volume and network interfaces; data volumes are kept. Instances in an Auto Scaling group, EKS worker nodes and instances with termination protection are refused. Use action plan first to see what is deleted and kept, then execute; execute again if it reports that the instance is still updating."
      memory      = 128
      timeout     = 30
      destructive = true
      environment = {}
      # The function checks EKS and Auto Scaling membership itself; IAM can't express those
      # refusals as tightly as Azure's resource-group scopes.
      statements = [
        # DescribeInstanceAttribute reads the termination protection the function refuses on.
        { actions = ["ec2:DescribeInstances", "ec2:DescribeInstanceAttribute", "ec2:DescribeNetworkInterfaces"], resources = ["*"], conditions = [] },
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
    node-pool-resize = {
      binary      = "node-pool-resize-aws"
      description = "Sets the desired size of an EKS managed node group (clusterName + nodegroupName) or an Auto Scaling group (autoScalingGroupName). Groups scaled by the cluster autoscaler, and Auto Scaling groups of EKS node groups, are refused. execute lowers the group's minimum or raises its maximum when the new size is outside them; plan reports newMin and newMax. Use action plan first to see the current size and the checks, then execute."
      memory      = 128
      timeout     = 30
      destructive = true
      environment = { MAX_NODE_COUNT = tostring(var.node_pool_max_count) }
      # Updating is limited to the node groups and Auto Scaling groups of the function's region
      # and account. Describing Auto Scaling groups can't be limited to resources.
      statements = [
        { actions = ["autoscaling:DescribeAutoScalingGroups"], resources = ["*"], conditions = [] },
        { actions = ["eks:DescribeNodegroup", "eks:UpdateNodegroupConfig"], resources = [local.nodegroup_arn], conditions = [] },
        { actions = ["autoscaling:UpdateAutoScalingGroup"], resources = [local.asg_arn], conditions = [] },
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/node-pool-resize.json")))
    }
    lb-delete = {
      binary      = "lb-delete-aws"
      description = "Deletes an orphaned Application or Network Load Balancer that Kubernetes created for a Service or Ingress, and then the target groups it left behind. Before calling it, confirm in the cluster that the Service or Ingress no longer exists, and pass the cluster and the Service as clusterName and serviceName (namespace/name). Load balancers with healthy targets, deletion protection or another owner are refused. Use action plan first; each execute makes one change, so execute again while the result reports remaining: true."
      memory      = 128
      timeout     = 60
      destructive = true
      environment = {}
      # Deleting is limited to load balancers and target groups created for Kubernetes, by the
      # AWS Load Balancer Controller or the Kubernetes cloud provider, in the function's region.
      # The function checks that they were created for the Service it is given. Describing can't
      # be limited to resources.
      statements = [
        {
          actions = [
            "elasticloadbalancing:DescribeLoadBalancers",
            "elasticloadbalancing:DescribeLoadBalancerAttributes",
            "elasticloadbalancing:DescribeTargetGroups",
            "elasticloadbalancing:DescribeTargetHealth",
            "elasticloadbalancing:DescribeTags",
          ]
          resources  = ["*"]
          conditions = []
        },
        { actions = ["elasticloadbalancing:DeleteLoadBalancer"], resources = [local.lb_arn], conditions = [local.controller_tag_condition] },
        { actions = ["elasticloadbalancing:DeleteLoadBalancer"], resources = [local.lb_arn], conditions = [local.service_name_tag_condition] },
        { actions = ["elasticloadbalancing:DeleteTargetGroup"], resources = [local.target_group_arn], conditions = [local.controller_tag_condition] },
        { actions = ["elasticloadbalancing:DeleteTargetGroup"], resources = [local.target_group_arn], conditions = [local.service_name_tag_condition] },
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/lb-delete.json")))
    }
    db-restore = {
      binary      = "db-restore-aws"
      description = "Restores an RDS DB instance to a point in time as a new instance. The source is never changed. Use action plan first to see the earliest and latest restorable times and the checks, then execute; call again with the same parameters to see the new instance's state and endpoint. Aurora isn't supported."
      memory      = 128
      timeout     = 30
      destructive = true
      environment = {}
      # Describing and listing tags can't be limited to resources. Restore also checks the
      # subnet group, parameter group and security groups it copies from the source, so
      # RestoreDBInstanceToPointInTime can't be limited to db:* alone.
      statements = [
        {
          actions = [
            "rds:DescribeDBInstances",
            "rds:ListTagsForResource",
          ]
          resources  = ["*"]
          conditions = []
        },
        {
          actions    = ["rds:RestoreDBInstanceToPointInTime"]
          resources  = ["*"]
          conditions = []
        },
        {
          actions    = ["rds:AddTagsToResource"]
          resources  = [local.db_instance_arn]
          conditions = []
        },
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/db-restore.json")))
    }
    ssh-access = {
      binary      = "ssh-access-aws"
      description = "Grants an IAM user or role short-lived SSH access to a Linux EC2 instance through SSM Session Manager, by attaching a temporary customer-managed policy until it expires, and returns the aws ssm start-session command. revoke: true removes the access right away. Use action plan first to check the instance, then execute."
      memory      = 128
      timeout     = 30
      destructive = true
      environment = { MAX_DURATION_MINUTES = tostring(var.ssh_access_max_minutes) }
      # CreatePolicy can't be limited to a path; deleting and tagging are limited to
      # /plural.sh/ssh-access/. Attaching is limited to users and roles in the account.
      # Describing instances and SSM registration, and listing policies, can't be limited to
      # resources.
      statements = [
        { actions = ["ec2:DescribeInstances"], resources = ["*"], conditions = [] },
        { actions = ["ssm:DescribeInstanceInformation"], resources = ["*"], conditions = [] },
        { actions = ["iam:ListPolicies", "iam:CreatePolicy"], resources = ["*"], conditions = [] },
        {
          actions = [
            "iam:DeletePolicy",
            "iam:GetPolicy",
            "iam:ListPolicyVersions",
            "iam:DeletePolicyVersion",
            "iam:TagPolicy",
            "iam:ListPolicyTags",
            "iam:ListEntitiesForPolicy",
          ]
          resources  = [local.iam_policy_arn]
          conditions = []
        },
        {
          actions    = ["iam:AttachUserPolicy", "iam:DetachUserPolicy"]
          resources  = [local.iam_user_arn, local.iam_policy_arn]
          conditions = []
        },
        {
          actions    = ["iam:AttachRolePolicy", "iam:DetachRolePolicy"]
          resources  = [local.iam_role_arn, local.iam_policy_arn]
          conditions = []
        },
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/ssh-access.json")))
    }
  }

  functions      = { for key, fn in local.catalog : key => fn if contains(var.functions, key) }
  unknown        = setsubtract(var.functions, keys(local.catalog))
  function_names = { for key, _ in local.functions : key => "${var.name}-${key}" }
  artifacts      = { for key, fn in local.functions : key => "${var.artifact_dir}/${var.artifact_version}/${fn.binary}.zip" }
}
