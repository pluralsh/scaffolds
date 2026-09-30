# Grants invoking the functions registered as workbench tools, and nothing else. Attach it to
# the IAM principal of the workbench cloud connection.
data "aws_iam_policy_document" "invoke" {
  count = length(local.tools) > 0 ? 1 : 0

  statement {
    sid       = "InvokeFunctions"
    actions   = ["lambda:InvokeFunction"]
    resources = [for key, _ in local.tools : aws_lambda_function.function[key].arn]
  }
}

resource "aws_iam_policy" "invoke" {
  count = length(local.tools) > 0 ? 1 : 0

  name        = "${var.name}-invoke"
  description = "Invoke the ${var.name} operational functions from Plural workbenches."
  policy      = data.aws_iam_policy_document.invoke[0].json
  tags        = var.tags
}

data "plural_cloud_connection" "workbench" {
  name = var.cloud_connection

  lifecycle {
    postcondition {
      condition     = self.cloud_provider == "AWS"
      error_message = "Cloud connection ${var.cloud_connection} is for ${self.cloud_provider}, not AWS."
    }
  }
}

# Tools of functions that change resources require human approval of every call.
resource "plural_workbench_tool" "function" {
  for_each = local.tools

  name                = replace(local.function_names[each.key], "-", "_")
  tool                = "LAMBDA"
  cloud_connection_id = data.plural_cloud_connection.workbench.id
  approval            = each.value.destructive

  configuration = {
    lambda = {
      lambda_arn   = aws_lambda_function.function[each.key].arn
      description  = each.value.description
      input_schema = each.value.schema
    }
  }
}
