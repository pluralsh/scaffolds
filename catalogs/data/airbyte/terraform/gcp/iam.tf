resource "google_service_account" "console_sa" {
  account_id = "console-sa"
  display_name = "Service account for Console GitHub actions"
}

resource "google_service_account_key" "default" {
  service_account_id = google_service_account.console_sa.name
}

data "aws_iam_policy_document" "airbyte" { // TODO
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

resource "aws_iam_policy_attachment" "airbyte-user" { // TODO
  name = "${local.cluster_name}-airbyte-policy"
  users = [aws_iam_user.airbyte.name]
  policy_arn = aws_iam_policy.airbyte.arn
}