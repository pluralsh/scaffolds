resource "aws_s3_bucket" "airbyte" {
  bucket         = var.airbyte_bucket
  force_destroy  = var.force_destroy_bucket
}

data "aws_iam_role" "postgres" {
  name = "${var.cluster_name}-postgres"
}
