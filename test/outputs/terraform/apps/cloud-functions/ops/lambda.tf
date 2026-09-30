data "aws_iam_policy_document" "assume" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}

data "aws_iam_policy_document" "function" {
  for_each = local.functions

  statement {
    sid       = "Logs"
    actions   = ["logs:CreateLogStream", "logs:PutLogEvents"]
    resources = ["${aws_cloudwatch_log_group.function[each.key].arn}:*"]
  }

  dynamic "statement" {
    for_each = each.value.statements

    content {
      actions   = statement.value.actions
      resources = statement.value.resources
    }
  }
}

resource "aws_iam_role" "function" {
  for_each = local.functions

  name               = local.function_names[each.key]
  assume_role_policy = data.aws_iam_policy_document.assume.json
  tags               = var.tags
}

resource "aws_iam_role_policy" "function" {
  for_each = local.functions

  name   = local.function_names[each.key]
  role   = aws_iam_role.function[each.key].id
  policy = data.aws_iam_policy_document.function[each.key].json
}

resource "aws_cloudwatch_log_group" "function" {
  for_each = local.functions

  name              = "/aws/lambda/${local.function_names[each.key]}"
  retention_in_days = var.log_retention_days
  tags              = var.tags
}

# The package is read from the directory the stack's init container downloads the release
# to. Lambda copies it on deploy, so nothing depends on the release afterwards.
resource "aws_lambda_function" "function" {
  for_each = local.functions

  function_name = local.function_names[each.key]
  description   = each.value.description
  role          = aws_iam_role.function[each.key].arn
  runtime       = "provided.al2023"
  handler       = "bootstrap"
  architectures = ["arm64"]
  memory_size   = each.value.memory
  timeout       = each.value.timeout

  filename = local.artifacts[each.key]
  # Guarded so that a missing package fails with the precondition below instead of a
  # function error.
  source_code_hash = fileexists(local.artifacts[each.key]) ? filebase64sha256(local.artifacts[each.key]) : null

  logging_config {
    log_format = "JSON"
    log_group  = aws_cloudwatch_log_group.function[each.key].name
  }

  tags = var.tags

  depends_on = [aws_iam_role_policy.function]

  lifecycle {
    precondition {
      condition     = fileexists(local.artifacts[each.key])
      error_message = "${local.artifacts[each.key]} not found. The stack's fetch-functions init container downloads it; check that it ran and that ${var.artifact_version} contains ${each.value.binary}.zip."
    }
  }
}
