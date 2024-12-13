
resource "aws_iam_policy" "airbyte" {
  name_prefix = "airbyte"
  description = "policy for the plural admin airbyte"
  policy      = data.aws_iam_policy_document.airbyte.json
}

resource "aws_iam_user" "airbyte" {
  name = "${data.plural_cluster.cluster.name}-airbyte"

  depends_on = [ plural_cluster.cluster ]
}

resource "aws_iam_access_key" "airbyte" {
  user = aws_iam_user.airbyte.name
}

data "aws_iam_policy_document" "airbyte" {
  statement {
    sid    = "admin"
    effect = "Allow"
    actions = ["s3:*"]

    resources = [
      "arn:aws:s3:::${var.airbyte_bucket}",
      "arn:aws:s3:::${var.airbyte_bucket}/*",
    ]
  }
}

resource "aws_iam_policy_attachment" "airbyte-user" {
  name = "${data.plural_cluster.cluster.name}-airbyte-policy"
  users = [aws_iam_user.airbyte.name]
  policy_arn = aws_iam_policy.airbyte.arn

  depends_on = [ plural_cluster.cluster ]
}
