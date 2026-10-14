use educoder_ui::{AppWindow, Controller};
use slint::ComponentHandle;
use slint::winit_030::{WinitWindowAccessor, winit::window::ResizeDirection};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let app = AppWindow::new()?;
    let controller = Controller::bind(&app, runtime.handle().clone());
    controller.initialize();
    let _web_host = educoder_ui::webview::WebHost::attach(&app, controller.clone());
    let weak = app.as_weak();
    app.on_drag_window(move || {
        if let Some(app) = weak.upgrade() {
            app.window().with_winit_window(|window| {
                let _ = window.drag_window();
            });
        }
    });
    let weak = app.as_weak();
    app.on_minimize_window(move || {
        if let Some(app) = weak.upgrade() {
            app.window()
                .with_winit_window(|window| window.set_minimized(true));
        }
    });
    let weak = app.as_weak();
    app.on_maximize_window(move || {
        if let Some(app) = weak.upgrade() {
            app.window()
                .with_winit_window(|window| window.set_maximized(!window.is_maximized()));
        }
    });
    let weak = app.as_weak();
    app.on_resize_window(move |direction| {
        if let Some(app) = weak.upgrade() {
            app.window().with_winit_window(|window| {
                let direction = match direction {
                    0 => ResizeDirection::West,
                    1 => ResizeDirection::East,
                    2 => ResizeDirection::South,
                    _ => ResizeDirection::North,
                };
                let _ = window.drag_resize_window(direction);
            });
        }
    });
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
