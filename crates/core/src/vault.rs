use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Error, Result, Session};

/// OS credential storage. No plaintext fallback. Windows/macOS are supported.
/// The account label is local metadata, not necessarily the platform login.
#[derive(Clone)]
pub struct SessionVault {
    account: String,
    lock: Arc<Mutex<()>>,
}

impl SessionVault {
    pub fn new(account: impl Into<String>) -> Self {
        Self {
            account: account.into(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn save(&self, session: &Session) -> Result<()> {
        let _guard = self.lock.lock().await;
        let account = self.account.clone();
        let secret = session.encode()?;
        tokio::task::spawn_blocking(move || save(&account, &secret))
            .await
            .map_err(|_| Error::Background)?
    }

    pub async fn load(&self) -> Result<Option<Session>> {
        let _guard = self.lock.lock().await;
        let account = self.account.clone();
        tokio::task::spawn_blocking(move || load(&account))
            .await
            .map_err(|_| Error::Background)?
    }

    pub async fn clear(&self) -> Result<()> {
        let _guard = self.lock.lock().await;
        let account = self.account.clone();
        tokio::task::spawn_blocking(move || clear(&account))
            .await
            .map_err(|_| Error::Background)?
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new("educoder-plus-plus", account).map_err(|_| Error::Vault)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn save(account: &str, secret: &str) -> Result<()> {
    entry(account)?
        .set_password(secret)
        .map_err(|_| Error::Vault)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn load(account: &str) -> Result<Option<Session>> {
    match entry(account)?.get_password() {
        Ok(secret) => Session::decode(&zeroize::Zeroizing::new(secret)).map(Some),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(Error::Vault),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn clear(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(Error::Vault),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn save(_: &str, _: &str) -> Result<()> {
    Err(Error::UnsupportedVault)
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn load(_: &str) -> Result<Option<Session>> {
    Err(Error::UnsupportedVault)
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn clear(_: &str) -> Result<()> {
    Err(Error::UnsupportedVault)
}
