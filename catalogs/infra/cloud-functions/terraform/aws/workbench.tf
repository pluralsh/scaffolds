# Grants invoking the deployed functions, and nothing else. Attach it to the IAM principal
# of the workbench cloud connection.
data "aws_iam_policy_document" "invoke" {
  statement {
    sid       = "InvokeFunctions"
    actions   = ["lambda:InvokeFunction"]
    resources = [for fn in aws_lambda_function.function : fn.arn]
  }
}

resource "aws_iam_policy" "invoke" {
  name        = "${var.name}-invoke"
  description = "Invoke the ${var.name} operational functions from Plural workbenches."
  policy      = data.aws_iam_policy_document.invoke.json
  tags        = var.tags
}

# The provider does not support `approval` yet, so every tool is invokable without human
# approval. Only register functions that change nothing until that is supported.
# TODO(PROD-5251): add `approval` to plural_workbench_tool in terraform-provider-plural and
# set it here for every function that changes resources.
# TODO(PROD-5251): fix the plural_cloud_connection data source (cloud_provider and
# configuration are required, so it cannot look connections up by name) and accept a
# connection name instead of cloud_connection_id.
resource "plural_workbench_tool" "function" {
  for_each = local.functions

  name                = replace(local.function_names[each.key], "-", "_")
  tool                = "LAMBDA"
  cloud_connection_id = var.cloud_connection_id

  configuration = {
    lambda = {
      lambda_arn   = aws_lambda_function.function[each.key].arn
      description  = each.value.description
      input_schema = each.value.schema
    }
  }
}
