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

# Code updates are driven by artifact_version, which is part of both the S3 key and the
# local file name. Release assets are immutable, so no source_code_hash is needed.
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

  filename  = try(local_file.artifact[each.key].filename, null)
  s3_bucket = local.download ? null : var.artifact_s3_bucket
  s3_key    = local.download ? null : local.artifact_key[each.key]

  logging_config {
    log_format = "JSON"
    log_group  = aws_cloudwatch_log_group.function[each.key].name
  }

  tags = var.tags

  depends_on = [aws_iam_role_policy.function]
}
