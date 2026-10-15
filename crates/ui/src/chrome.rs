//! Frameless controls and a real rounded native window, including Windows 10.
use crate::AppWindow;
use slint::{
    ComponentHandle, Timer, TimerMode,
    winit_030::{WinitWindowAccessor, winit::window::ResizeDirection},
};
use std::time::Duration;

pub struct WindowChrome {
    _timer: Timer,
}

/// Select before creating any Slint window. Winit's transparent Windows mode
/// enables DWM blur, which can expose zero-alpha pixels from software surfaces.
pub fn initialize_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(|attributes| {
            #[cfg(windows)]
            let attributes = attributes.with_transparent(false);
            attributes
        })
        .select()
}

impl WindowChrome {
    pub fn attach(app: &AppWindow) -> Self {
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
        let weak = app.as_weak();
        let timer = Timer::default();
        let mut previous = None;
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            app.window().with_winit_window(|window| {
                let size = window.inner_size();
                let state = (
                    size.width,
                    size.height,
                    window.scale_factor(),
                    window.is_maximized(),
                );
                if previous != Some(state) {
                    app.set_window_maximized(state.3);
                    #[cfg(windows)]
                    round_window(window);
                    previous = Some(state);
                }
            });
        });
        Self { _timer: timer }
    }
}

#[cfg(windows)]
fn round_window(window: &slint::winit_030::winit::window::Window) {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        return;
    }
    let hwnd = handle.hwnd.get() as *mut std::ffi::c_void;
    unsafe {
        if window.is_maximized() {
            SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
            return;
        }
        let diameter = (24.0 * window.scale_factor()).round() as i32;
        let region = CreateRoundRectRgn(
            0,
            0,
            size.width as i32 + 1,
            size.height as i32 + 1,
            diameter,
            diameter,
        );
        if !region.is_null() && SetWindowRgn(hwnd, region, 1) == 0 {
            // On success Windows owns the region. Only failed transfers are freed here.
            DeleteObject(region);
        }
    }
}
