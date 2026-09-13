test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-node-feature-discovery-mgmt.yaml --validate
  plural pr contracts --file test/contracts-node-feature-discovery-workload.yaml --validate
