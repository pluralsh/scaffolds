# cert-manager DNS-01 fleet setup

Choose the cloud and your ACME contact email in the catalog. The generated controller and issuer GlobalServices target workload clusters of that cloud's distribution: EKS, AKS or GKE. Both retain the `role: workload` tag filter. The issuer is named `dns01` and uses the Let's Encrypt production endpoint; use a staging issuer when testing issuance.

The scaffold selects the DNS provider and fills in your inputs. Plural resolves the remaining `cluster.metadata` expressions separately for each target cluster. Configure that metadata and cloud permissions before deploying:

| Cloud | Required catalog inputs | Required target-cluster metadata |
| --- | --- | --- |
| AWS | `region` | `iam.cert_manager`: IAM role ARN |
| Azure | `dnsZone`, `resourceGroupName` | `subscription_id`: DNS zone subscription ID; `iam.cert_manager`: managed identity client ID |
| GCP | No additional inputs | `project`: DNS zone project ID; `iam.cert_manager`: Google service-account email |

The Kubernetes service account is `cert-manager` in namespace `cert-manager`. Cloud credentials are obtained through workload identity; do not put access keys, tokens or passwords in these metadata fields.

## Cloud prerequisites

- **AWS:** Configure IRSA trust for `system:serviceaccount:cert-manager:cert-manager` using the EKS OIDC issuer. Grant the role Route53 access to create and remove DNS-01 TXT records in the intended hosted zone and read the zone/change information required by cert-manager. The chosen `region` is passed to the Route53 solver.
- **Azure:** Enable AKS OIDC and workload identity. Federate the managed identity with the AKS issuer, subject `system:serviceaccount:cert-manager:cert-manager` and audience `api://AzureADTokenExchange`. Grant it DNS Zone Contributor on the public DNS zone. The zone, resource group and subscription must refer to the same DNS configuration. The controller pod receives the workload identity label; its service-account annotation and issuer `managedIdentity.clientID` use the same client ID. Webhook and cainjector pods do not need the controller's Azure identity label.
- **GCP:** Enable Workload Identity Federation for GKE and bind the Kubernetes service account to the annotated Google service account. Grant that Google service account the Cloud DNS permissions required for the intended project and managed zone. `cluster.metadata.project` must name the DNS project's ID.

See the provider guides for exact trust and role policies: [Route53](https://cert-manager.io/docs/configuration/acme/dns01/route53/), [AzureDNS](https://cert-manager.io/docs/configuration/acme/dns01/azuredns/), and [CloudDNS](https://cert-manager.io/docs/configuration/acme/dns01/google/).

## Verify before requesting certificates

Inspect `helm/certmanager.yaml.liquid` and the generated `services/cert-manager/issuers/dns01.yaml.liquid`. Confirm your email, exactly one intended DNS provider, and the deferred metadata values after cluster rendering. The chart remains on the existing `v1.x.x` range; no chart upgrade is required by this template correction.

Deploy the controller and its CRDs before the issuer can reconcile. Check the controller's readiness and the `dns01` ClusterIssuer status, then validate with a staging issuer and a domain you control. Restart the controller pods after changing workload identity service-account annotations so admission can inject updated identity settings.

Local scaffold, Helm and schema validation checks do not prove cloud permissions, federation, ACME registration, DNS propagation or successful live certificate issuance. The catalog does not create the provider-side identities or grant their cloud roles.
