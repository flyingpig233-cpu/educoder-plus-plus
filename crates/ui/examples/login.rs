//! Smoke test of the official login view, empty-session feedback and reopening.
//! Does not restore credentials, enter an account, or submit a platform login.
use educoder_ui::{AppWindow, Controller, login::LoginHost};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::Cell, rc::Rc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    educoder_ui::chrome::initialize_backend()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let app = AppWindow::new()?;
    let controller = Controller::bind(&app, runtime.handle().clone());
    let _chrome = educoder_ui::chrome::WindowChrome::attach(&app);
    let _login = LoginHost::attach(&app, controller);
    app.show()?;
    app.set_login_open(true);
    app.invoke_start_browser_login();
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let weak = app.as_weak();
    let timer = Timer::default();
    let mut stage = 0;
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(app) = weak.upgrade() else { return };
        let visible = login_view_visible(&app);
        match stage {
            0 if visible => {
                app.invoke_finish_browser_login();
                stage = 1;
            }
            1 if !app.get_login_reading() && !app.get_error_message().is_empty() => {
                assert!(!app.get_has_session());
                assert!(!app.get_busy());
                app.invoke_cancel_browser_login();
                assert!(!app.get_login_open());
                app.set_login_open(true);
                app.invoke_start_browser_login();
                stage = 2;
            }
            2 if visible => {
                app.invoke_cancel_browser_login();
                stage = 3;
            }
            3 if !visible => {
                assert!(!app.get_browser_login_open());
                result.set(true);
                let _ = slint::quit_event_loop();
            }
            _ => {}
        }
    });
    let timeout = Timer::default();
    timeout.start(TimerMode::SingleShot, Duration::from_secs(15), || {
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop()?;
    if !passed.get() {
        return Err("login view smoke test did not complete".into());
    }
    println!("Official login view / asynchronous empty session / cancel and reopen: true");
    Ok(())
}

#[cfg(windows)]
fn login_view_visible(app: &AppWindow) -> bool {
    use slint::winit_030::{
        WinitWindowAccessor,
        winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumChildWindows, GetClassNameW, IsWindowVisible},
    };
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        let mut class = [0u16; 64];
        let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
        if String::from_utf16_lossy(&class[..length.max(0) as usize]) == "WRY_WEBVIEW"
            && unsafe { IsWindowVisible(hwnd) } != 0
        {
            unsafe { *(data as *mut bool) = true };
        }
        1
    }
    app.window()
        .with_winit_window(|window| {
            let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
                return false;
            };
            let mut visible = false;
            unsafe {
                EnumChildWindows(
                    handle.hwnd.get() as HWND,
                    Some(visit),
                    (&mut visible as *mut bool) as LPARAM,
                )
            };
            visible
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn login_view_visible(_: &AppWindow) -> bool {
    false
}
