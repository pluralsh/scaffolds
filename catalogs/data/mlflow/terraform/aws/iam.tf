
resource "aws_iam_policy" "mlflow" {
  name_prefix = "mlflow"
  description = "policy for the plural admin mlflow"
  policy      = data.aws_iam_policy_document.mlflow.json
}

resource "aws_iam_user" "mlflow" {
  name = "${data.plural_cluster.cluster.name}-mlflow"

  depends_on = [ data.plural_cluster.cluster ]
}

resource "aws_iam_access_key" "mlflow" {
  user = aws_iam_user.mlflow.name
}

data "aws_iam_policy_document" "mlflow" {
  statement {
    sid    = "admin"
    effect = "Allow"
    actions = ["s3:*"]

    resources = [
      "arn:aws:s3:::${var.mlflow_bucket}",
      "arn:aws:s3:::${var.mlflow_bucket}/*",
    ]
  }
}

resource "aws_iam_policy_attachment" "mlflow-user" {
  name = "${data.plural_cluster.cluster.name}-mlflow-policy"
  users = [aws_iam_user.mlflow.name]
  policy_arn = aws_iam_policy.mlflow.arn

  depends_on = [ data.plural_cluster.cluster ]
}
