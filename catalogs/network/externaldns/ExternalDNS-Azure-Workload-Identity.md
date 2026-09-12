# ExternalDNS on AKS with workload identity

These steps apply when this catalog's cloud is `azure`. AWS and GCP continue to use their existing service-account annotations; they do not need this Azure configuration.

## Before deploying

For every AKS cluster selected by the GlobalService:

1. Enable the cluster's OIDC issuer and Microsoft Entra Workload ID. The workload identity webhook must be running.
2. Create a user-assigned managed identity. Set `cluster.metadata.iam.external_dns` in Plural to its **client ID**, not its resource ID or object ID. The generated Kubernetes service account uses this value in its `azure.workload.identity/client-id` annotation.
3. Grant that identity the Azure **DNS Zone Contributor** role scoped to the public DNS zone it should manage, and the **Reader** role scoped to the resource group containing that zone. If managing multiple zones, grant access to each intended zone. Configure the zone resource group and subscription in the JSON below. The domain filter limits which DNS names ExternalDNS reconciles; it does not replace Azure role assignments.
4. Create a federated identity credential on the managed identity, with the AKS OIDC issuer, audience `api://AzureADTokenExchange`, and subject `system:serviceaccount:external-dns:<service-account-name>`. Inspect the rendered chart for the actual service-account name; for Helm release `external-dns` with the pinned chart defaults, it is `external-dns`. The namespace must match the GlobalService's `external-dns` namespace.
5. Create the configuration Secret below in that namespace before starting ExternalDNS. Use the same Secret name in every targeted AKS cluster, but each cluster may have different JSON contents. Enter that name in the catalog's required `azureConfigSecret` field.

## Azure configuration

Create a local `azure.json` using your subscription ID and DNS zone's resource group:

```json
{
  "subscriptionId": "00000000-0000-0000-0000-000000000000",
  "resourceGroup": "dns-resource-group",
  "useWorkloadIdentityExtension": true
}
```

This configuration contains resource identifiers, not a client secret, password, or access token. Authentication uses the projected workload identity token. On each intended AKS cluster, explicitly select its context and create the Secret, for example:

```sh
kubectl --context YOUR_AKS_CONTEXT create namespace external-dns --dry-run=client -o yaml | kubectl --context YOUR_AKS_CONTEXT apply -f -
kubectl --context YOUR_AKS_CONTEXT -n external-dns create secret generic external-dns-azure --from-file=azure.json=./azure.json --dry-run=client -o yaml | kubectl --context YOUR_AKS_CONTEXT apply -f -
```

Restart the ExternalDNS pods after changing the configuration Secret so the process reloads its Azure configuration.

Set `azureConfigSecret` to `external-dns-azure` for this example. Keep `useWorkloadIdentityExtension` enabled in the JSON. Do not add client-secret credentials or enable managed-identity authentication in place of workload identity.

The catalog passes the existing Secret name through `azure.secretName`. This mounts its `azure.json` at `/etc/kubernetes/azure.json`, avoiding the chart's default node-host configuration and its generated Azure configuration path. The pinned Bitnami chart 8.3.8 emits invalid JSON with a trailing comma when generating workload-identity-only credentials, so supplying a valid existing configuration Secret is intentional.

The workload identity opt-in label belongs on the **pod template**, not the service account. The catalog sets `podLabels.azure.workload.identity/use` to the string `"true"`; the service account retains the client-ID annotation. This lets the AKS webhook inject its environment variables and projected token volume.

## Verify deployment

Inspect the generated Helm values and rendered Deployment before applying. Confirm the pod label, annotated service account, named Secret volume and `/etc/kubernetes/` mount. On AKS, check that the admitted pod includes workload identity environment variables and its projected token volume, then check ExternalDNS logs for successful Azure authentication and DNS zone discovery. Do not print token contents. Confirm DNS updates only in the intended zone with an application you control.

Local contract and Helm rendering checks verify the generated resources. They do not validate your Azure role assignments, federation, webhook configuration, image availability, or live DNS reconciliation.

## References

- [ExternalDNS Azure workload identity setup](https://kubernetes-sigs.github.io/external-dns/v0.15.0/docs/tutorials/azure/)
- [Azure workload identity pod labels and service-account annotations](https://azure.github.io/azure-workload-identity/docs/topics/service-account-labels-and-annotations.html)
