test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-trust-manager-standard.yaml --validate
  plural pr contracts --file test/contracts-trust-manager-custom.yaml --validate
