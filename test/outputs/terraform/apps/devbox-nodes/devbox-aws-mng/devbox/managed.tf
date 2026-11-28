locals {
  managed = var.provisioner == "managed-node-group" ? 1 : 0
}

data "aws_subnet" "nodes" {
  for_each = local.managed == 1 ? toset(local.subnet_ids) : toset([])
  id       = each.value
}

locals {
  # One node group per zone: a group spanning zones is one Auto Scaling group, and the cluster autoscaler can't choose
  # which zone it launches into, so a worker pinned to its homes' zone could wait on a node in another.
  zone_subnets = { for id, subnet in data.aws_subnet.nodes : subnet.availability_zone => id... }

  # Lets the cluster autoscaler build a zone's node from its template while the group is at zero.
  autoscaler_tags = merge(
    { for k, v in local.labels : "k8s.io/cluster-autoscaler/node-template/label/${k}" => v },
    { for t in values(local.taints) : "k8s.io/cluster-autoscaler/node-template/taint/${t.key}" => "${t.value}:NoSchedule" },
  )
  zone_autoscaler_tags = merge([
    for zone in keys(local.zone_subnets) : {
      for k, v in merge(local.autoscaler_tags, { "k8s.io/cluster-autoscaler/node-template/label/topology.kubernetes.io/zone" = zone }) :
      "${zone}/${k}" => { zone = zone, key = k, value = v }
    }
  ]...)
}

resource "aws_iam_role" "nodes" {
  count = local.managed
  name  = local.node_role_name

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ec2.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy_attachment" "nodes" {
  for_each = local.managed == 1 ? toset([
    "arn:aws:iam::aws:policy/AmazonEKSWorkerNodePolicy",
    "arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy",
    "arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly",
    "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore",
  ]) : toset([])

  role       = aws_iam_role.nodes[0].name
  policy_arn = each.value
}

resource "aws_launch_template" "nodes" {
  count       = local.managed
  name_prefix = "${local.cluster_name}-${var.pool}-"

  # Setting any security group drops the cluster primary group EKS attaches by default, so it's listed too.
  vpc_security_group_ids = local.node_security_group_ids

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 1
  }

  block_device_mappings {
    device_name = "/dev/xvda"
    ebs {
      volume_size           = var.boot_disk_size
      volume_type           = "gp3"
      encrypted             = true
      delete_on_termination = true
    }
  }

  dynamic "block_device_mappings" {
    for_each = var.disk_mode == "network" ? [1] : []
    content {
      device_name = "/dev/xvdb"
      ebs {
        volume_size           = var.data_disk_size
        volume_type           = "gp3"
        iops                  = 6000
        throughput            = 500
        encrypted             = true
        delete_on_termination = true
      }
    }
  }

  tag_specifications {
    resource_type = "instance"
    tags          = { "plural.sh/devbox-pool" = var.pool }
  }
}

resource "aws_eks_node_group" "devbox" {
  for_each        = local.zone_subnets
  cluster_name    = local.cluster_name
  node_group_name = "${var.pool}-${var.sandbox_class}-${each.key}"
  node_role_arn   = aws_iam_role.nodes[0].arn
  subnet_ids      = each.value
  ami_type        = "AL2023_x86_64_STANDARD"
  instance_types  = local.instance_types
  capacity_type   = "ON_DEMAND"
  labels          = local.labels

  launch_template {
    id      = aws_launch_template.nodes[0].id
    version = aws_launch_template.nodes[0].latest_version
  }

  # Zones start empty and each may take the whole pool. The runner's minReplicas and maxReplicas bound the workers
  # across zones and decide which zone keeps the warm ones; a per-zone minimum would keep idle nodes in every zone.
  scaling_config {
    min_size     = 0
    desired_size = 0
    max_size     = var.max_nodes
  }

  dynamic "taint" {
    for_each = local.taints
    content {
      key    = taint.value.key
      value  = taint.value.value
      effect = "NO_SCHEDULE"
    }
  }

  update_config {
    max_unavailable = 1
  }

  lifecycle {
    ignore_changes = [scaling_config[0].desired_size]

    precondition {
      condition     = var.sandbox_class != "microvm"
      error_message = "EKS managed node groups drop the launch template's nested virtualization CPU option, use karpenter for microvm."
    }
  }

  depends_on = [aws_iam_role_policy_attachment.nodes]
}

resource "aws_autoscaling_group_tag" "devbox" {
  for_each               = local.zone_autoscaler_tags
  autoscaling_group_name = aws_eks_node_group.devbox[each.value.zone].resources[0].autoscaling_groups[0].name

  tag {
    key                 = each.value.key
    value               = each.value.value
    propagate_at_launch = false
  }
}
