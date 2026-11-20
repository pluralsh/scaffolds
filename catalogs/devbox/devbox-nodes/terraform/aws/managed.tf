locals {
  managed = var.provisioner == "managed-node-group" ? 1 : 0
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
  count           = local.managed
  cluster_name    = local.cluster_name
  node_group_name = "${var.pool}-${var.sandbox_class}"
  node_role_arn   = aws_iam_role.nodes[0].arn
  subnet_ids      = local.subnet_ids
  ami_type        = "AL2023_x86_64_STANDARD"
  instance_types  = local.instance_types
  capacity_type   = "ON_DEMAND"
  labels          = local.labels

  launch_template {
    id      = aws_launch_template.nodes[0].id
    version = aws_launch_template.nodes[0].latest_version
  }

  scaling_config {
    min_size     = var.min_nodes
    desired_size = var.min_nodes
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
