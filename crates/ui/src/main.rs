slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    AppWindow::new()?.run()
}
