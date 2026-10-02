# EventBridge invokes ssh-access every 5 minutes with {"pluralExpire": true} so it can remove
# expired customer-managed policies under /plural.sh/ssh-access/, the same cleanup every
# execute does for the principal it was called with.
resource "aws_cloudwatch_event_rule" "ssh_access_expire" {
  count = contains(keys(local.functions), "ssh-access") ? 1 : 0

  name                = "${var.name}-ssh-access-expire"
  description         = "Removes expired ssh-access IAM policies every 5 minutes."
  schedule_expression = "rate(5 minutes)"
  tags                = var.tags
}

resource "aws_cloudwatch_event_target" "ssh_access_expire" {
  count = contains(keys(local.functions), "ssh-access") ? 1 : 0

  rule      = aws_cloudwatch_event_rule.ssh_access_expire[0].name
  target_id = "ssh-access"
  arn       = aws_lambda_function.function["ssh-access"].arn
  input     = jsonencode({ pluralExpire = true })
}

resource "aws_lambda_permission" "ssh_access_expire" {
  count = contains(keys(local.functions), "ssh-access") ? 1 : 0

  statement_id  = "AllowExecutionFromEventBridge"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.function["ssh-access"].function_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.ssh_access_expire[0].arn
}
