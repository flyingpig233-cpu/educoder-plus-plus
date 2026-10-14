//! Async Educoder platform access, independent of the desktop UI.
mod client;
mod dto;
mod error;
pub mod models;
mod session;
mod signing;
mod vault;

pub use client::{ClientOptions, EducoderClient};
pub use error::{Error, Result};
pub use session::Session;
pub use vault::SessionVault;
