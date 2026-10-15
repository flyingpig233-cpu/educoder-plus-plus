//! Frameless controls and a real rounded native window, including Windows 10.
use crate::AppWindow;
use slint::{
    ComponentHandle, Timer, TimerMode,
    winit_030::{WinitWindowAccessor, winit::window::ResizeDirection},
};
use std::time::Duration;

pub struct WindowChrome {
    _timer: Timer,
    #[cfg(windows)]
    _frame: std::rc::Rc<std::cell::RefCell<Option<native_frame::FrameGuard>>>,
}

/// Select before creating any Slint window. Winit's transparent Windows mode
/// enables DWM blur, which can expose zero-alpha pixels from software surfaces.
pub fn initialize_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(|attributes| {
            #[cfg(windows)]
            let attributes = attributes.with_transparent(false).with_decorations(false);
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
        #[cfg(windows)]
        let frame = std::rc::Rc::new(std::cell::RefCell::new(None));
        #[cfg(windows)]
        let native_frame = frame.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            app.window().with_winit_window(|window| {
                #[cfg(windows)]
                if native_frame.borrow().is_none() {
                    *native_frame.borrow_mut() = native_frame::FrameGuard::install(window);
                    app.window().request_redraw();
                }
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
                    if native_frame.borrow().is_some() {
                        round_window(window);
                    }
                    previous = Some(state);
                }
            });
        });
        Self {
            _timer: timer,
            #[cfg(windows)]
            _frame: frame,
        }
    }
}

#[cfg(windows)]
mod native_frame {
    use slint::winit_030::winit::{
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GWL_STYLE, GetWindowLongW, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE,
                SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongW, SetWindowPos, WM_NCACTIVATE,
                WM_NCCALCSIZE, WM_NCDESTROY, WM_NCPAINT, WM_STYLECHANGING, WS_CAPTION,
            },
        },
    };

    const SUBCLASS_ID: usize = 0x45445543;
    pub(super) struct FrameGuard(HWND);

    impl FrameGuard {
        pub fn install(window: &Window) -> Option<Self> {
            let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
                return None;
            };
            let hwnd = handle.hwnd.get() as HWND;
            // Installed and removed on the Slint/Winit UI thread; no borrowed data
            // crosses the callback boundary. Winit retains all client-area handling.
            if unsafe { SetWindowSubclass(hwnd, Some(frame_proc), SUBCLASS_ID, 0) } == 0 {
                return None;
            }
            unsafe {
                let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
                SetWindowLongW(hwnd, GWL_STYLE, (style & !WS_CAPTION) as i32);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            Some(Self(hwnd))
        }
    }

    impl Drop for FrameGuard {
        fn drop(&mut self) {
            unsafe {
                RemoveWindowSubclass(self.0, Some(frame_proc), SUBCLASS_ID);
            }
        }
    }

    unsafe extern "system" fn frame_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        _: usize,
    ) -> LRESULT {
        match message {
            // Winit reapplies its native styles when maximizing/restoring. Keep
            // WS_THICKFRAME and the system commands, but never restore a caption.
            WM_STYLECHANGING if wparam as i32 == GWL_STYLE => unsafe {
                if let Some(style) = (lparam as *mut STYLESTRUCT).as_mut() {
                    style.styleNew &= !WS_CAPTION;
                }
                DefSubclassProc(hwnd, message, wparam, lparam)
            },
            // Winit normally keeps WS_CAPTION and extends the client area through
            // WM_NCCALCSIZE. Suppress classic frame painting as well, since window
            // regions and activation changes can trigger that fallback path.
            WM_NCPAINT => 0,
            WM_NCCALCSIZE if wparam == 0 => 0,
            // Forward activation so Winit still updates focus. -1 tells
            // DefWindowProc to update activation without painting a native frame.
            WM_NCACTIVATE => unsafe { DefSubclassProc(hwnd, message, wparam, -1) },
            WM_NCDESTROY => unsafe {
                RemoveWindowSubclass(hwnd, Some(frame_proc), id);
                DefSubclassProc(hwnd, message, wparam, lparam)
            },
            _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
        }
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
