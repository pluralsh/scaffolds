test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-n8n-aws.yaml --validate
  plural pr contracts --file test/contracts-n8n-azure.yaml --validate
  plural pr contracts --file test/contracts-n8n-gcp.yaml --validate
