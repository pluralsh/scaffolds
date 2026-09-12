test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-opencost-credentials.yaml --validate
  plural pr contracts --file test/contracts-opencost-existing-secret.yaml --validate
