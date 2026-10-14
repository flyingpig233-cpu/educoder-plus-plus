use educoder_ui::{AppWindow, Controller};
use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let app = AppWindow::new()?;
    let controller = Controller::bind(&app, runtime.handle().clone());
    controller.restore();
    app.run()?;
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    Ok(())
}
