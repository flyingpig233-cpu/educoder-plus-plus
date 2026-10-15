mod avatar;
pub mod chrome;
mod controller;
pub mod login;
mod preferences;
mod progress;
mod rendering;
pub mod webview;
mod workbench;

pub use controller::Controller;
slint::include_modules!();
