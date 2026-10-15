use educoder_ui::{AppWindow, Controller};
use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let app = AppWindow::new()?;
    let controller = Controller::bind(&app, runtime.handle().clone());
    controller.initialize();
    let _web_host = educoder_ui::webview::WebHost::attach(&app, controller.clone());
    let _chrome = educoder_ui::chrome::WindowChrome::attach(&app);
    let close_controller = controller.clone();
    app.window().on_close_requested(move || {
        close_controller.request_close();
        slint::CloseRequestResponse::KeepWindowShown
    });
    controller.restore();
    app.run()?;
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    Ok(())
}
