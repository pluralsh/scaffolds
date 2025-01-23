data "aws_iam_policy_document" "osbuilder" {
  statement {
    sid    = "admin"
    effect = "Allow"
    actions = ["s3:*"]

    resources = [
      "arn:aws:s3:::${var.osbuilder_bucket}",
      "arn:aws:s3:::${var.osbuilder_bucket}/*",
    ]
  }
}

resource "aws_iam_policy" "osbuilder" {
  name_prefix = "osbuilder"
  description = "policy for the plural osbuilder registry"
  policy      = data.aws_iam_policy_document.osbuilder.json
}

resource "aws_iam_user" "osbuilder" {
  name = "${data.plural_cluster.cluster.name}-osbuilder"

  depends_on = [ data.plural_cluster.cluster ]
}

resource "aws_iam_access_key" "osbuilder" {
  user = aws_iam_user.osbuilder.name
}

resource "aws_iam_policy_attachment" "osbuilder-user" {
  name = "${data.plural_cluster.cluster.name}-osbuilder-policy"
  users = [aws_iam_user.osbuilder.name]
  policy_arn = aws_iam_policy.osbuilder.arn

  depends_on = [ data.plural_cluster.cluster ]
}
