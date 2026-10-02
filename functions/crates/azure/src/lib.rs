//! Azure helpers: managed identity credentials and a minimal Azure Resource Manager client.

pub mod arm;
#[cfg(feature = "mock")]
pub mod mock;

use std::sync::Arc;

use azure_core::credentials::TokenCredential;
use functions_core::Error;

/// Token scope for Azure Resource Manager.
pub const ARM_SCOPE: &str = "https://management.azure.com/.default";

/// Credential of the function app's system-assigned managed identity.
#[cfg(not(feature = "az-cli"))]
pub fn credential() -> Result<Arc<dyn TokenCredential>, Error> {
    Ok(azure_identity::ManagedIdentityCredential::new(None).map_err(provider_error)?)
}

/// Credential of the Azure CLI's signed-in account, for running functions locally against real
/// Azure. Only builds with the `az-cli` feature use it; release packages never do.
#[cfg(feature = "az-cli")]
pub fn credential() -> Result<Arc<dyn TokenCredential>, Error> {
    Ok(azure_identity::AzureCliCredential::new(None).map_err(provider_error)?)
}

/// Converts an Azure SDK error into [`Error::Provider`].
pub fn provider_error(err: azure_core::Error) -> Error {
    Error::provider(err)
}
