//! Azure helpers: managed identity credentials, the identity functions run as, and a
//! minimal Azure Resource Manager client.

pub mod arm;

use std::sync::Arc;

use azure_core::credentials::TokenCredential;
pub use azure_identity::ManagedIdentityCredential;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use functions_core::Error;
use serde::{Deserialize, Serialize};

/// Token scope for Azure Resource Manager.
pub const ARM_SCOPE: &str = "https://management.azure.com/.default";

/// Credential of the function app's system-assigned managed identity.
pub fn credential() -> Result<Arc<ManagedIdentityCredential>, Error> {
    ManagedIdentityCredential::new(None).map_err(provider_error)
}

/// Converts an Azure SDK error into [`Error::Provider`].
pub fn provider_error(err: azure_core::Error) -> Error {
    Error::provider(err)
}

/// Identity a token was issued to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub tenant_id: Option<String>,
    pub object_id: Option<String>,
    pub client_id: Option<String>,
    /// Resource ID of the managed identity, when the token was issued to one.
    pub identity_resource_id: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    tid: Option<String>,
    oid: Option<String>,
    appid: Option<String>,
    xms_mirid: Option<String>,
}

/// Resolves the identity `credential` authenticates as from the claims of an ARM token.
///
/// Getting a token needs no role assignments, so this also verifies the managed identity
/// setup without granting any permissions.
pub async fn caller_identity(credential: &dyn TokenCredential) -> Result<Identity, Error> {
    let token = credential
        .get_token(&[ARM_SCOPE], None)
        .await
        .map_err(provider_error)?;
    identity_from_jwt(token.token.secret())
}

fn identity_from_jwt(jwt: &str) -> Result<Identity, Error> {
    let payload = jwt
        .split('.')
        .nth(1)
        .ok_or_else(|| Error::provider("access token is not a JWT"))?;
    let claims: Claims = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .map_err(Error::provider)
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(Error::provider))?;

    Ok(Identity {
        tenant_id: claims.tid,
        object_id: claims.oid,
        client_id: claims.appid,
        identity_resource_id: claims.xms_mirid,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn jwt(claims: serde_json::Value) -> String {
        let encode = |v: &serde_json::Value| URL_SAFE_NO_PAD.encode(v.to_string());
        format!(
            "{}.{}.signature",
            encode(&json!({"alg": "RS256"})),
            encode(&claims)
        )
    }

    #[test]
    fn reads_identity_claims() {
        let token = jwt(json!({
            "tid": "tenant",
            "oid": "object",
            "appid": "client",
            "xms_mirid": "/subscriptions/s/resourcegroups/rg/providers/Microsoft.Web/sites/app",
            "aud": "https://management.azure.com"
        }));

        assert_eq!(
            identity_from_jwt(&token).unwrap(),
            Identity {
                tenant_id: Some("tenant".into()),
                object_id: Some("object".into()),
                client_id: Some("client".into()),
                identity_resource_id: Some(
                    "/subscriptions/s/resourcegroups/rg/providers/Microsoft.Web/sites/app".into()
                ),
            }
        );
    }

    #[test]
    fn tolerates_missing_claims() {
        assert_eq!(
            identity_from_jwt(&jwt(json!({}))).unwrap(),
            Identity::default()
        );
    }

    #[test]
    fn rejects_non_jwt_tokens() {
        assert!(identity_from_jwt("opaque").is_err());
    }
}
