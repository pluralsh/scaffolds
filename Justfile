test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-grafana-neither.yaml --validate
  plural pr contracts --file test/contracts-grafana-tempo.yaml --validate
  plural pr contracts --file test/contracts-grafana-plural.yaml --validate
  plural pr contracts --file test/contracts-grafana-both.yaml --validate
