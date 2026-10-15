//! Async Educoder platform access, independent of the desktop UI.
pub mod answers;
pub mod cache;
mod client;
mod dto;
mod error;
pub mod models;
pub mod runner;
mod session;
mod signing;
mod vault;
pub mod workspace;

pub use client::{ClientOptions, EducoderClient};
pub use error::{Error, Result};
pub use session::{BrowserCookie, Session};
pub use vault::SessionVault;
