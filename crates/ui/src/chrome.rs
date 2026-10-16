//! Frameless controls and a real rounded native window, including Windows 10.
use crate::{AppWindow, ConfettiParticle};
use slint::{
    ComponentHandle, Timer, TimerMode,
    winit_030::{WinitWindowAccessor, winit::window::ResizeDirection},
};
use std::time::{Duration, Instant};

fn confetti() -> Vec<ConfettiParticle> {
    let palette = [0x70d6a1, 0x82b6ff, 0xf7cd6c, 0xd6a0ee, 0xf197ae, 0x81d7d4];
    let mut seed = 0x45445543u32;
    let mut random = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 16777216.0
    };
    (0..90)
        .map(|i| ConfettiParticle {
            x: random(),
            drift: random() * 2.0 - 1.0,
            delay: random() * 0.23,
            speed: 1.5 + random() * 1.1,
            spin: random() * 1440.0 - 720.0,
            size: 5.0 + random() * 4.0,
            color: slint::Color::from_rgb_u8(
                (palette[i % palette.len()] >> 16) as u8,
                (palette[i % palette.len()] >> 8) as u8,
                palette[i % palette.len()] as u8,
            ),
        })
        .collect()
}

fn reduce_motion() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut enabled = 1i32;
        return SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        ) != 0
            && enabled == 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub struct WindowChrome {
    _timer: Timer,
    #[cfg(windows)]
    _frame: std::rc::Rc<std::cell::RefCell<Option<native_frame::FrameGuard>>>,
}

/// Decodes the bundled application icon into a winit bitmap. Returns `None`
/// when the PNG cannot be read, in which case the window keeps the default icon.
fn app_icon() -> Option<slint::winit_030::winit::window::Icon> {
    const ICON: &[u8] = include_bytes!("../assets/app-icon.png");
    let icon = image::load_from_memory(ICON).ok()?.into_rgba8();
    let (width, height) = icon.dimensions();
    slint::winit_030::winit::window::Icon::from_rgba(icon.into_raw(), width, height).ok()
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
        app.set_confetti(slint::ModelRc::new(slint::VecModel::from(confetti())));
        app.set_reduce_motion(reduce_motion());
        #[cfg(windows)]
        let transition = std::rc::Rc::new(std::cell::Cell::new(None::<Instant>));
        let weak = app.as_weak();
        app.on_drag_window(move || {
            if let Some(app) = weak.upgrade() {
                if app.get_window_transitioning() {
                    return;
                }
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
        #[cfg(windows)]
        let maximize_transition = transition.clone();
        app.on_maximize_window(move || {
            if let Some(app) = weak.upgrade() {
                if app.get_window_transitioning() {
                    return;
                }
                app.window().with_winit_window(|window| {
                    if window.fullscreen().is_some() {
                        return;
                    }
                    app.invoke_hide_account_menu();
                    #[cfg(windows)]
                    {
                        native_frame::prepare_transition(window, !app.get_reduce_motion());
                        if !app.get_reduce_motion() {
                            maximize_transition.set(Some(Instant::now()));
                            app.set_window_transitioning(true);
                        }
                    }
                    window.set_maximized(!window.is_maximized());
                    #[cfg(windows)]
                    if app.get_reduce_motion() {
                        native_frame::finish_transition(window);
                    }
                    app.set_window_maximized(window.is_maximized());
                    crate::rendering::repaint_all(&app);
                });
            }
        });
        let weak = app.as_weak();
        app.on_resize_window(move |direction| {
            if let Some(app) = weak.upgrade() {
                app.window().with_winit_window(|window| {
                    // The UI state is polled, so recheck the native state before
                    // starting a resize. Windows must not enter its resize loop
                    // while maximized or fullscreen.
                    if app.get_window_transitioning()
                        || window.is_maximized()
                        || window.fullscreen().is_some()
                    {
                        return;
                    }
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
        // The winit window only exists once the event loop runs, so the taskbar
        // and Alt+Tab icon is applied on the first timer tick.
        let window_icon = std::cell::Cell::new(app_icon());
        let mut previous = None;
        let mut previous_page = app.get_page();
        let mut page_started = None::<Instant>;
        let mut celebration_id = app.get_celebration_id();
        let mut celebration_started = None::<Instant>;
        #[cfg(windows)]
        let frame = std::rc::Rc::new(std::cell::RefCell::new(None));
        #[cfg(windows)]
        let native_frame = frame.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_page() != previous_page {
                previous_page = app.get_page();
                page_started = Some(Instant::now());
            }
            if let Some(start) = page_started {
                let phase = (start.elapsed().as_secs_f32() / 0.18).min(1.0);
                app.set_page_reveal(if app.get_reduce_motion() {
                    1.0
                } else {
                    1.0 - (1.0 - phase).powi(3)
                });
                if phase >= 1.0 {
                    page_started = None;
                }
            }
            if celebration_id != app.get_celebration_id() {
                celebration_id = app.get_celebration_id();
                celebration_started = Some(Instant::now());
            }
            if let Some(start) = celebration_started {
                let phase = (start.elapsed().as_secs_f32()
                    / if app.get_reduce_motion() { 1.5 } else { 5.0 })
                .min(1.0);
                app.set_celebration_progress(phase);
                if phase >= 1.0 || !app.get_celebration_active() {
                    app.set_celebration_active(false);
                    celebration_started = None;
                }
            }
            app.window().with_winit_window(|window| {
                if let Some(icon) = window_icon.take() {
                    window.set_window_icon(Some(icon));
                }
                #[cfg(windows)]
                {
                    if let Some(started) = transition.get()
                        && (app.get_reduce_motion()
                            || started.elapsed() >= Duration::from_millis(250))
                    {
                        transition.set(None);
                        app.set_window_transitioning(false);
                        native_frame::finish_transition(window);
                        round_window(window);
                        crate::rendering::repaint_all(&app);
                    }
                }
                let dark = match app.get_theme_mode() {
                    1 => false,
                    2 => true,
                    _ => window.theme().map_or(app.get_dark_mode(), |theme| {
                        theme == slint::winit_030::winit::window::Theme::Dark
                    }),
                };
                if app.get_dark_mode() != dark {
                    app.set_dark_mode(dark);
                    crate::rendering::repaint_all(&app);
                }
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
                    window.fullscreen().is_some(),
                );
                if previous != Some(state) {
                    app.set_window_maximized(state.3);
                    app.set_window_fullscreen(state.4);
                    #[cfg(windows)]
                    if native_frame.borrow().is_some() && !app.get_window_transitioning() {
                        round_window(window);
                        crate::rendering::repaint_all(&app);
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
        Graphics::{
            Dwm::{
                DWMNCRP_USEWINDOWSTYLE, DWMWA_NCRENDERING_POLICY, DWMWA_TRANSITIONS_FORCEDISABLED,
                DwmSetWindowAttribute,
            },
            Gdi::SetWindowRgn,
        },
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

    /// Frame styles are cached by Windows. Commit caption changes before the
    /// size transition, and after restoring the frameless state, rather than
    /// letting the next maximize/restore apply a stale non-client frame.
    unsafe fn set_caption(hwnd: HWND, enabled: bool) {
        unsafe {
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let updated = if enabled {
                style | WS_CAPTION
            } else {
                style & !WS_CAPTION
            };
            if updated != style {
                SetWindowLongW(hwnd, GWL_STYLE, updated as i32);
                refresh_frame(hwnd);
            }
        }
    }

    unsafe fn refresh_frame(hwnd: HWND) {
        unsafe {
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
    }

    pub fn prepare_transition(window: &Window, animate: bool) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as HWND;
        unsafe {
            SetWindowSubclass(hwnd, Some(frame_proc), SUBCLASS_ID, usize::from(animate));
            if animate {
                set_caption(hwnd, true);
            }
            let disabled = i32::from(!animate);
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED as u32,
                (&disabled as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            );
            // A fixed window region clips DWM's transformed surface. Restore
            // corners after the transition instead of resizing the region per frame.
            SetWindowRgn(hwnd, std::ptr::null_mut(), 0);
        }
    }

    pub fn finish_transition(window: &Window) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as HWND;
        unsafe {
            SetWindowSubclass(hwnd, Some(frame_proc), SUBCLASS_ID, 0);
            set_caption(hwnd, false);
        }
    }

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
                // The compositor caption style is only needed while transitioning.
                // Removing it at rest prevents classic frame painting on Windows 10.
                SetWindowLongW(hwnd, GWL_STYLE, (style & !WS_CAPTION) as i32);
                let policy = DWMNCRP_USEWINDOWSTYLE;
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_NCRENDERING_POLICY as u32,
                    (&policy as *const i32).cast(),
                    std::mem::size_of::<i32>() as u32,
                );
                refresh_frame(hwnd);
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
        animating: usize,
    ) -> LRESULT {
        match message {
            // Retain compositor eligibility when Winit reapplies its styles.
            WM_STYLECHANGING if wparam as i32 == GWL_STYLE => unsafe {
                if let Some(style) = (lparam as *mut STYLESTRUCT).as_mut() {
                    if animating != 0 {
                        style.styleNew |= WS_CAPTION;
                    } else {
                        style.styleNew &= !WS_CAPTION;
                    }
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
        if window.is_maximized() || window.fullscreen().is_some() {
            SetWindowRgn(hwnd, std::ptr::null_mut(), 0);
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
        if !region.is_null() && SetWindowRgn(hwnd, region, 0) == 0 {
            // On success Windows owns the region. Only failed transfers are freed here.
            DeleteObject(region);
        }
    }
}
