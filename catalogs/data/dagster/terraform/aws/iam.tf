data "aws_iam_policy_document" "dagster" {
  statement {
    sid    = "admin"
    effect = "Allow"
    actions = ["s3:*"]

    resources = [
      "arn:aws:s3:::${var.dagster_bucket}",
      "arn:aws:s3:::${var.dagster_bucket}/*",
    ]
  }
}

resource "aws_iam_policy" "dagster" {
  name_prefix = "dagster"
  description = "policy for the plural admin dagster"
  policy      = data.aws_iam_policy_document.dagster.json
}

resource "aws_iam_user" "dagster" {
  name = "${data.plural_cluster.cluster.name}-dagster"

  depends_on = [ data.plural_cluster.cluster ]

}

resource "aws_iam_access_key" "dagster" {
  user = aws_iam_user.dagster.name
}

resource "aws_iam_policy_attachment" "dagster-user" {
  name = "${data.plural_cluster.cluster.name}-dagster-policy"
  users = [aws_iam_user.dagster.name]
  policy_arn = aws_iam_policy.dagster.arn
}
