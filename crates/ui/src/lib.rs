mod avatar;
pub mod chrome;
mod controller;
mod preferences;
pub mod webview;
mod workbench;

pub use controller::Controller;
slint::include_modules!();
