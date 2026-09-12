test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-external-dns-aws.yaml --validate
  plural pr contracts --file test/contracts-external-dns-azure.yaml --validate
  plural pr contracts --file test/contracts-external-dns-gcp.yaml --validate
