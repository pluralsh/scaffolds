data "aws_eks_cluster" "cluster" {
  name = local.eks_cluster_name
}

resource "aws_iam_policy" "bedrock" {
  name_prefix = "bedrock"
  description = "Bedrock permissions for AI Proxy in ${local.eks_cluster_name}"
  policy      = <<-POLICY
    {
      "Version": "2012-10-17",
      "Statement": [
        {
          "Effect": "Allow",
          "Action": [
            "bedrock:InvokeModel",
            "bedrock:InvokeModelWithResponseStream",
            "bedrock:ListFoundationModels",
            "bedrock:GetModelInvocationMetrics"
          ],
          "Resource": "*"
        },
        {
          "Effect": "Allow",
          "Action": [
            "bedrock:ListCustomModels",
            "bedrock:GetCustomModel",
            "bedrock:ListProvisionedModelThroughputs",
            "bedrock:GetProvisionedModelThroughput"
          ],
          "Resource": "*"
        },
        {
          "Effect": "Allow",
          "Action": [
            "bedrock:InvokeModelWithResponseStream",
            "bedrock:InvokeModel",
            "bedrock:InvokeModelAsync"
          ],
          "Resource": "arn:aws:bedrock:*:*:foundation-model/*"
        },
        {
          "Effect": "Allow",
          "Action": [
            "bedrock:InvokeModel",
            "bedrock:InvokeModelWithResponseStream"
          ],
          "Resource": "arn:aws:bedrock:*:*:custom-model/*"
        },
        {
          "Effect": "Allow",
          "Action": [
            "bedrock:Converse",
            "bedrock:ListConversations",
            "bedrock:GetConversation"
          ],
          "Resource": "*"
        }
      ]
    }
  POLICY
}

module "assumable_role_bedrock" {
  source  = "terraform-aws-modules/iam/aws//modules/iam-assumable-role-with-oidc"
  version = "5.52.2"

  create_role      = true
  role_name        = "${local.eks_cluster_name}-bedrock"
  provider_url     = replace(data.aws_eks_cluster.cluster.identity[0].oidc[0].issuer, "https://", "")
  role_policy_arns = [aws_iam_policy.bedrock.arn]

  oidc_fully_qualified_subjects = [
    "system:serviceaccount:bedrock:bedrock-sa",
  ]
}
