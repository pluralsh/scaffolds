//! Azure helpers: managed identity credentials and a minimal Azure Resource Manager client.

pub mod arm;

use std::sync::Arc;

pub use azure_identity::ManagedIdentityCredential;
use functions_core::Error;

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
