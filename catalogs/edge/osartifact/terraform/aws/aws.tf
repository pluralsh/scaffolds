data "aws_caller_identity" "current" {}

data "aws_eks_cluster" "eks_cluster" {
  name = data.plural_cluster.cluster.name
}