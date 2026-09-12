test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-certmanager-aws.yaml --validate
  plural pr contracts --file test/contracts-certmanager-azure.yaml --validate
  plural pr contracts --file test/contracts-certmanager-gcp.yaml --validate
