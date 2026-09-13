//! API keys in the operating system credential store.
//!
//! A key never enters `settings.json`. That file records which provider is selected and nothing more;
//! the key is read from the keyring at the moment a request is made. Removing a key deletes the
//! credential rather than blanking a field, so a key that has disappeared from the interface has
//! disappeared from the machine.

use keyring::Entry;

use crate::error::{AppError, AppResult};

/// One service name for the whole application; the account distinguishes what is stored.
const SERVICE: &str = "com.threadbox.desktop";

/// Where a language model provider's key lives. Keys for providers that are not currently selected
/// are kept, so switching back to one does not mean typing its key again.
pub(crate) fn language_model_account(provider: &str) -> String {
    format!("language-model/{provider}")
}

pub(crate) fn store(account: &str, secret: &str) -> AppResult<()> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(AppError::InvalidInput("An API key is required".into()));
    }
    Entry::new(SERVICE, account)?.set_password(secret)?;
    Ok(())
}

pub(crate) fn read(account: &str) -> AppResult<Option<String>> {
    match Entry::new(SERVICE, account)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Deleting a key that is not there is not a failure: the requested state is reached either way.
pub(crate) fn delete(account: &str) -> AppResult<()> {
    match Entry::new(SERVICE, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Whether a key is configured. The interface has to be able to show this without the key itself ever
/// crossing the boundary, so nothing here returns the secret.
pub(crate) fn is_present(account: &str) -> bool {
    matches!(read(account), Ok(Some(_)))
}
