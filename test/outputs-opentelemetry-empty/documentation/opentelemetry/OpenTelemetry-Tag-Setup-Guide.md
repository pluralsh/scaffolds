# Plural OpenTelemetry Stack

This catalog provides a production-ready deployment of the OpenTelemetry observability stack using Plural. By leveraging Plural's infrastructure management, it simplifies and enhances OpenTelemetry collection for monitoring and troubleshooting distributed systems.

## Cluster tag filter

The optional `tags` input applies the same cluster tag filter to the collector and eBPF GlobalServices. Use comma-separated `key=value` pairs, for example `release=1.0,enabled=true,tier=01`. Keys and values are serialized as strings, preserving numeric-looking values such as `01` and boolean-looking values such as `true`.

Omit `tags` or supply an empty string to leave both GlobalServices without a tag filter. The input retains its simple comma/equals grammar: use nonempty keys and values without embedded commas or equals signs. This change does not add an escaping syntax or change the chart configurations.

## Key Features

- **Low Overhead Observability**:
  Capture system and application-level telemetry with minimal performance impact using efficient eBPF-based probes.

- **Dynamic Observability**:
  Automatically collects telemetry data for both network and application-level activities.

- **Seamless Backend Integration**:
  Ships telemetry data to OpenTelemetry-compatible backends for processing and visualization.

- **Prometheus Support**:
  Built-in support for exporting eBPF-collected metrics to Prometheus, enabling detailed monitoring and alerting.

- **Kubernetes Native Deployment**:
  Fully compatible with Kubernetes and Plural’s platform, allowing for easy configuration and deployment using Custom Resource Definitions (CRDs).

To update settings or add more configurations, simply modify the relevant sections in your Plural project’s YAML files.

## Contributing & Extending Functionality

If you’d like to contribute or enhance this project, feel free to submit issues, improvements, or new features to the [Plural scaffolds repository](https://github.com/pluralsh/scaffolds). Contributions are always welcome!

For more details on the OpenTelemetry project, check out the [official OpenTelemetry documentation](https://opentelemetry.io/).
