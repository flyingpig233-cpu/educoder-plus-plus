//! Monaco and Markdown run in an offline child WebView inside the Slint window.
use crate::{AppWindow, Controller};
use include_dir::{Dir, include_dir};
use slint::{ComponentHandle, Model, Timer, TimerMode, winit_030::WinitWindowAccessor};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use wry::{
    Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web/dist");

pub struct WebHost {
    _timer: Timer,
    _view: Rc<RefCell<Option<WebView>>>,
    ready: Rc<Cell<bool>>,
}

impl WebHost {
    pub fn attach(app: &AppWindow, controller: Arc<Controller>) -> Self {
        let view = Rc::new(RefCell::new(None::<WebView>));
        let ready = Rc::new(Cell::new(false));
        let failed = Rc::new(Cell::new(false));
        let ui = app.as_weak();
        let target = view.clone();
        let host_ready = ready.clone();
        let timer = Timer::default();
        let mut last_bounds = None;
        let mut last_theme = None;
        let mut last_connection = None;
        let mut last_visible = false;
        let mut last_page = None;
        let mut last_celebration = 0;
        let mut celebration_visible = false;
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = ui.upgrade() else { return };
            if last_page != Some(app.get_page()) {
                last_page = Some(app.get_page());
                crate::rendering::repaint_all(&app);
            }
            let visible = app.get_page() == 2 && !app.get_login_open();
            if visible && target.borrow().is_none() && !failed.get() {
                let ipc_ui = ui.clone();
                let ipc_ready = ready.clone();
                let built = app.window().with_winit_window(|window| {
                    let path = educoder_core::cache::data_dir()
                        .ok()
                        .map(|root| root.join("webview"));
                    let mut context = wry::WebContext::new(path);
                    let builder = WebViewBuilder::new_with_web_context(&mut context)
                        .with_custom_protocol("educoder".into(), |_, request| {
                            asset_response(request.uri().path())
                        })
                        .with_url("educoder://localhost/index.html")
                        .with_visible(false)
                        .with_background_color((28, 28, 28, 255))
                        .with_navigation_handler(|url| trusted_url(&url))
                        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
                        .with_download_started_handler(|_, _| false)
                        .with_permission_handler(|permission| match permission {
                            wry::PermissionKind::ClipboardRead => wry::PermissionResponse::Default,
                            _ => wry::PermissionResponse::Deny,
                        })
                        .with_ipc_handler(move |request| {
                            if !trusted_url(&request.uri().to_string()) {
                                return;
                            }
                            let action = serde_json::from_str::<serde_json::Value>(request.body())
                                .ok()
                                .and_then(|v| {
                                    v.get("action").and_then(|s| s.as_str()).map(str::to_owned)
                                });
                            if action.as_deref() == Some("ready") {
                                ipc_ready.set(true);
                            }
                            if let Some(app) = ipc_ui.upgrade() {
                                if action.as_deref() == Some("dismiss-account-menu") {
                                    app.invoke_hide_account_menu();
                                } else {
                                    app.invoke_web_event(request.body().as_str().into());
                                }
                            }
                        });
                    builder.build_as_child(window)
                });
                if let Some(result) = built {
                    match result {
                        Ok(webview) => {
                            *target.borrow_mut() = Some(webview);
                        }
                        Err(_) => {
                            failed.set(true);
                            app.set_error_message(
                                "代码工作区启动失败，请检查系统 WebView2 运行时。".into(),
                            );
                            app.set_page(0);
                        }
                    }
                }
            }
            let binding = target.borrow();
            let Some(webview) = binding.as_ref() else {
                return;
            };
            let bounds = (
                app.get_web_x(),
                app.get_web_y(),
                app.get_web_width().max(1.0),
                app.get_web_height().max(1.0),
            );
            if last_bounds != Some(bounds) {
                let _ = webview.set_bounds(Rect {
                    position: LogicalPosition::new(bounds.0 as f64, bounds.1 as f64).into(),
                    size: LogicalSize::new(bounds.2 as f64, bounds.3 as f64).into(),
                });
                last_bounds = Some(bounds);
                if visible {
                    crate::rendering::repaint_all(&app);
                }
            }
            if visible != last_visible && webview.set_visible(visible).is_ok() {
                last_visible = visible;
                crate::rendering::repaint_all(&app);
            }
            if !ready.get() {
                return;
            }
            let theme = (app.get_dark_mode(), app.get_accent_index(), app.get_reduce_motion());
            if last_theme != Some(theme) {
                let accent = match (theme.0, theme.1) {
                    (true, 1) => "#8ab4f8",
                    (false, 1) => "#3569b5",
                    (true, 2) => "#c9a7ff",
                    (false, 2) => "#7950b8",
                    (true, _) => "#eeeeee",
                    (false, _) => "#202020",
                };
                let data = serde_json::json!({"type":"theme","dark":theme.0,"accent":accent,"reduce_motion":theme.2});
                let _ = webview.evaluate_script(&format!("window.educoder.receive({data})"));
                last_theme = Some(theme);
            }
            if last_connection != Some(app.get_connected()) {
                let data = serde_json::json!({"type":"connection","connected":app.get_connected()});
                let _ = webview.evaluate_script(&format!("window.educoder.receive({data})"));
                last_connection = Some(app.get_connected());
            }
            let messages = controller.drain_web_messages();
            if !messages.is_empty()
                && let Ok(data) = serde_json::to_string(&messages)
            {
                let _ = webview.evaluate_script(&format!(
                    "for(const message of {data}) window.educoder.receive(message);"
                ));
            }
            if last_celebration!=app.get_celebration_id()&&app.get_celebration_active() {
                last_celebration=app.get_celebration_id();celebration_visible=true;
                let particles=app.get_confetti().iter().map(|p|serde_json::json!({"x":p.x,"drift":p.drift,"delay":p.delay,"speed":p.speed,"spin":p.spin,"size":p.size,"color":format!("#{:02x}{:02x}{:02x}",p.color.red(),p.color.green(),p.color.blue())})).collect::<Vec<_>>();
                let scale=app.window().scale_factor();let size=app.window().size();
                let data=serde_json::json!({"type":"celebration","id":last_celebration,"game":app.get_celebration_game().as_str(),"width":size.width as f32/scale,"height":size.height as f32/scale,"left":app.get_web_x(),"top":46,"reduce_motion":app.get_reduce_motion(),"particles":particles});
                let _=webview.evaluate_script(&format!("window.educoder.receive({data})"));
            }
            if celebration_visible&&!app.get_celebration_active(){celebration_visible=false;let _=webview.evaluate_script("window.educoder.receive({type:'stop-celebration'})");}
        });
        Self {
            _timer: timer,
            _view: view,
            ready: host_ready,
        }
    }

    /// Used by the synthetic desktop integration preview; never loads an account.
    pub fn evaluate_when_ready(&self, script: &str) -> bool {
        if !self.ready.get() {
            return false;
        }
        self._view
            .borrow()
            .as_ref()
            .is_some_and(|view| view.evaluate_script(script).is_ok())
    }
}

fn trusted_url(url: &str) -> bool {
    url == "educoder://localhost/index.html"
        || url == "http://educoder.localhost/index.html"
        || url == "https://educoder.localhost/index.html"
}

fn asset_response(path: &str) -> wry::http::Response<Cow<'static, [u8]>> {
    let path = path.trim_start_matches('/');
    let content_type = if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".ttf") {
        "font/ttf"
    } else {
        "application/octet-stream"
    };
    match ASSETS.get_file(path) {
        Some(file) => wry::http::Response::builder()
            .header("Content-Type", content_type)
            .header("Cache-Control", "public, max-age=31536000")
            .header("X-Content-Type-Options", "nosniff")
            .body(Cow::Borrowed(file.contents()))
            .expect("asset response"),
        None => wry::http::Response::builder()
            .status(404)
            .body(Cow::Borrowed(&b"Not found"[..]))
            .expect("asset response"),
    }
}
