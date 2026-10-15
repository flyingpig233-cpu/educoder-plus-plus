//! User-driven official login. Remote pages receive no application IPC bridge.
use crate::{AppWindow, Controller};
use educoder_core::{BrowserCookie, Session};
use slint::{ComponentHandle, Timer, TimerMode, winit_030::WinitWindowAccessor};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use wry::{
    Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

const LOGIN_URL: &str = "https://www.educoder.net/user/login";
const API_URL: &str = "https://data.educoder.net/api/";

pub struct LoginHost {
    _timer: Timer,
    _view: Rc<RefCell<Option<WebView>>>,
    generation: Rc<Cell<u64>>,
}

impl LoginHost {
    pub fn attach(app: &AppWindow, controller: Arc<Controller>) -> Self {
        let view = Rc::new(RefCell::new(None::<WebView>));
        let generation = Rc::new(Cell::new(0u64));
        let weak = app.as_weak();
        app.on_start_browser_login(move || {
            if let Some(app) = weak.upgrade() {
                #[cfg(windows)]
                {
                    app.set_error_message("".into());
                    app.set_browser_login_open(true);
                }
                #[cfg(not(windows))]
                app.set_error_message(
                    "此平台暂未接入网页会话读取，请使用 Cookie 文件导入。".into(),
                );
            }
        });
        let weak = app.as_weak();
        let cancel_view = view.clone();
        let cancel_generation = generation.clone();
        app.on_cancel_browser_login(move || {
            if let Some(app) = weak.upgrade() {
                if app.get_busy() {
                    return;
                }
                cancel_generation.set(cancel_generation.get().wrapping_add(1));
                cancel_view.borrow_mut().take();
                app.set_browser_login_open(false);
                app.set_login_reading(false);
                app.set_login_open(false);
                crate::rendering::repaint_all(&app);
            }
        });
        let weak = app.as_weak();
        let finish_view = view.clone();
        let finish_generation = generation.clone();
        app.on_finish_browser_login(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_busy() || app.get_login_reading() {
                return;
            }
            let binding = finish_view.borrow();
            let Some(view) = binding.as_ref() else { return };
            app.set_login_reading(true);
            app.set_error_message("".into());
            let epoch = finish_generation.get();
            let generation = finish_generation.clone();
            let ui = weak.clone();
            let controller = controller.clone();
            let requested = request_cookies(view, move |result| {
                let Some(app) = ui.upgrade() else { return };
                if generation.get() != epoch
                    || !app.get_login_open()
                    || !app.get_browser_login_open()
                {
                    return;
                }
                app.set_login_reading(false);
                match result {
                    Ok(cookies) => match Session::from_browser_cookies(&cookies) {
                        Ok(session) => controller.import_browser_session(session),
                        Err(_) => app.set_error_message(
                            "尚未取得有效会话，请先在官方页面完成登录，再点击「已完成登录」。"
                                .into(),
                        ),
                    },
                    Err(()) => app.set_error_message(
                        "无法读取登录会话，请重试或使用 Cookie 文件导入。".into(),
                    ),
                }
            });
            if requested.is_err() {
                app.set_login_reading(false);
                app.set_error_message("无法读取登录会话，请重试。".into());
            }
        });
        let weak = app.as_weak();
        let target = view.clone();
        let lifetime_generation = generation.clone();
        let timer = Timer::default();
        let mut last_bounds = None;
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            if !app.get_login_open() || !app.get_browser_login_open() {
                if target.borrow_mut().take().is_some() {
                    generation.set(generation.get().wrapping_add(1));
                    app.set_login_reading(false);
                    app.set_browser_login_open(false);
                    last_bounds = None;
                    crate::rendering::repaint_all(&app);
                }
                return;
            }
            if target.borrow().is_none() {
                last_bounds = None;
                let profile = match educoder_core::cache::data_dir() {
                    Ok(path) => path.join("login-browser"),
                    Err(_) => {
                        app.set_browser_login_open(false);
                        app.set_error_message("无法初始化登录窗口，请检查用户数据目录。".into());
                        return;
                    }
                };
                let result = app.window().with_winit_window(|window| {
                    let mut context = wry::WebContext::new(Some(profile));
                    WebViewBuilder::new_with_web_context(&mut context)
                        .with_incognito(true)
                        .with_url(LOGIN_URL)
                        .with_visible(false)
                        .with_background_color((255, 255, 255, 255))
                        .with_navigation_handler(allowed_login_url)
                        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
                        .with_download_started_handler(|_, _| false)
                        .with_permission_handler(|_| wry::PermissionResponse::Deny)
                        .build_as_child(window)
                });
                match result {
                    Some(Ok(webview)) => {
                        disable_password_saving(&webview);
                        *target.borrow_mut() = Some(webview);
                    }
                    _ => {
                        app.set_browser_login_open(false);
                        app.set_error_message(
                            "登录窗口启动失败，请检查 WebView2 运行时，或使用 Cookie 文件导入。"
                                .into(),
                        );
                        return;
                    }
                }
            }
            let binding = target.borrow();
            let Some(view) = binding.as_ref() else { return };
            let size = app.window().size().to_logical(app.window().scale_factor());
            let bounds = (size.width, size.height);
            if last_bounds != Some(bounds) {
                let _ = view.set_bounds(Rect {
                    position: LogicalPosition::new(40.0, 124.0).into(),
                    size: LogicalSize::new(
                        (size.width - 80.0).max(1.0) as f64,
                        (size.height - 220.0).max(1.0) as f64,
                    )
                    .into(),
                });
                let _ = view.set_visible(true);
                last_bounds = Some(bounds);
            }
        });
        Self {
            _timer: timer,
            _view: view,
            generation: lifetime_generation,
        }
    }
}

impl Drop for LoginHost {
    fn drop(&mut self) {
        self._timer.stop();
        self.generation.set(self.generation.get().wrapping_add(1));
        self._view.borrow_mut().take();
    }
}

fn allowed_login_url(url: String) -> bool {
    let Ok(url) = wry::http::Uri::try_from(url.as_str()) else {
        return false;
    };
    url.scheme_str() == Some("https")
        && url
            .host()
            .is_some_and(|host| host == "educoder.net" || host.ends_with(".educoder.net"))
        && url
            .authority()
            .is_some_and(|authority| !authority.as_str().contains('@'))
        && url.port_u16().is_none_or(|port| port == 443)
}

#[cfg(windows)]
fn disable_password_saving(view: &WebView) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows::core::Interface;
    use wry::WebViewExtWindows;
    unsafe {
        if let Ok(settings) = view
            .webview()
            .Settings()
            .and_then(|settings| settings.cast::<ICoreWebView2Settings4>())
        {
            let _ = settings.SetIsPasswordAutosaveEnabled(false);
            let _ = settings.SetIsGeneralAutofillEnabled(false);
        }
    }
}

#[cfg(not(windows))]
fn disable_password_saving(_: &WebView) {}

#[cfg(windows)]
fn request_cookies(
    view: &WebView,
    callback: impl Fn(std::result::Result<Vec<BrowserCookie>, ()>) + 'static,
) -> std::result::Result<(), ()> {
    use webview2_com::{
        GetCookiesCompletedHandler, Microsoft::Web::WebView2::Win32::ICoreWebView2_2, take_pwstr,
    };
    use windows::core::{BOOL, HSTRING, Interface, PCWSTR, PWSTR};
    use wry::WebViewExtWindows;
    let webview = view.webview().cast::<ICoreWebView2_2>().map_err(|_| ())?;
    unsafe {
        let manager = webview.CookieManager().map_err(|_| ())?;
        let url = HSTRING::from(API_URL);
        manager
            .GetCookies(
                PCWSTR::from_raw(url.as_ptr()),
                &GetCookiesCompletedHandler::create(Box::new(move |status, list| {
                    let result = (|| -> windows::core::Result<Vec<BrowserCookie>> {
                        status?;
                        let mut result = Vec::new();
                        if let Some(list) = list {
                            let mut count = 0;
                            list.Count(&mut count)?;
                            for index in 0..count {
                                let cookie = list.GetValueAtIndex(index)?;
                                let mut name = PWSTR::null();
                                cookie.Name(&mut name)?;
                                let name = take_pwstr(name);
                                let mut domain = PWSTR::null();
                                cookie.Domain(&mut domain)?;
                                let domain = take_pwstr(domain);
                                let mut path = PWSTR::null();
                                cookie.Path(&mut path)?;
                                let path = take_pwstr(path);
                                let mut secure = BOOL::default();
                                cookie.IsSecure(&mut secure)?;
                                let mut session = BOOL::default();
                                cookie.IsSession(&mut session)?;
                                let mut expires = 0.0;
                                cookie.Expires(&mut expires)?;
                                let mut value = PWSTR::null();
                                cookie.Value(&mut value)?;
                                result.push(BrowserCookie {
                                    include_subdomains: domain.starts_with('.'),
                                    domain,
                                    path,
                                    secure: secure.as_bool(),
                                    expires: if session.as_bool() {
                                        0
                                    } else {
                                        expires.max(1.0) as u64
                                    },
                                    name,
                                    value: take_pwstr(value),
                                });
                            }
                        }
                        Ok(result)
                    })();
                    callback(result.map_err(|_| ()));
                    Ok(())
                })),
            )
            .map_err(|_| ())
    }
}

#[cfg(not(windows))]
fn request_cookies(
    _: &WebView,
    _: impl Fn(std::result::Result<Vec<BrowserCookie>, ()>) + 'static,
) -> std::result::Result<(), ()> {
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_navigation_is_scoped_to_official_https_hosts() {
        for url in [
            LOGIN_URL,
            "https://data.educoder.net/api/",
            "https://www.educoder.net:443/",
        ] {
            assert!(allowed_login_url(url.into()));
        }
        for url in [
            "http://www.educoder.net/",
            "https://educoder.net.evil.example/",
            "https://evil.example/?next=educoder.net",
            "https://user@www.educoder.net/",
            "https://www.educoder.net:444/",
            "javascript:alert(1)",
            "file:///tmp/login",
        ] {
            assert!(!allowed_login_url(url.into()));
        }
    }
}
