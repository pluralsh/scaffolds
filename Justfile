test:
  plural pr contracts --file test/contracts.yaml --validate
  plural pr contracts --file test/contracts-opentelemetry-tagged.yaml --validate
  plural pr contracts --file test/contracts-opentelemetry-omitted.yaml --validate
  plural pr contracts --file test/contracts-opentelemetry-empty.yaml --validate
