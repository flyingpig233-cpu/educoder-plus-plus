//! Exercises the actual Slint + WebView2 child, offline Monaco and native IPC.
//! Synthetic data only; no session is restored and no platform is contacted.
#[cfg(windows)]
#[path = "support/native_capture.rs"]
mod native_capture;
use educoder_ui::{AppWindow, Controller, webview::WebHost};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    educoder_ui::chrome::initialize_backend()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let app = AppWindow::new()?;
    let _chrome = educoder_ui::chrome::WindowChrome::attach(&app);
    let controller = Controller::bind(&app, runtime.handle().clone());
    let host = WebHost::attach(&app, controller);
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let replies = Rc::new(RefCell::new(Vec::<serde_json::Value>::new()));
    let ipc_replies = replies.clone();
    let ipc_ui = app.as_weak();
    app.on_web_event(move |body| {
        let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if value["action"] == "answers" || value["action"] == "draft" {
            let data = if value["action"] == "answers" {
                serde_json::json!({"answer":{"title":"Native preview","status":"public_samples_passed","editable_paths":["main.rs"],"files":[{"path":"answer/demo.rs","content":"fn main() { /* native answer */ }"}]},"cached":false,"stale":false})
            } else { serde_json::json!({}) };
            ipc_replies.borrow_mut().push(serde_json::json!({"reply_to":value["id"],"ok":true,"data":data}));
        }
        if value["action"] == "native-self-test" {
            result.set(
                value["payload"]["ok"] == true
                    && ipc_ui
                        .upgrade()
                        .is_some_and(|app| !app.get_account_menu_open()),
            );
            println!("Native offline Monaco / Markdown / answer copy-import / IPC: {}", result.get());
        }
    });
    app.set_page(2);
    app.show()?;
    let tick = Timer::default();
    let mut sent = false;
    let weak = app.as_weak();
    let browser_passed = passed.clone();
    let chrome_passed = Rc::new(Cell::new(false));
    let native_passed = chrome_passed.clone();
    let mut stage = 0;
    let mut settle_ticks = 0;
    tick.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(app) = weak.upgrade() else { return };
        for reply in replies.borrow_mut().drain(..) {
            host.evaluate_when_ready(&format!("window.educoder.receive({reply})"));
        }
        match stage {
            0 => {
                app.invoke_show_account_menu();
                assert!(app.get_account_menu_open());
                stage = 1;
            }
            1 => {
                app.invoke_hide_account_menu();
                assert!(!app.get_account_menu_open());
                #[cfg(windows)]
                capture_title_bar(&app, "titlebar-native-normal");
                #[cfg(windows)]
                assert!(rounded_region(&app));
                app.invoke_maximize_window();
                stage = 2;
            }
            2 if app.get_window_maximized() => {
                settle_ticks += 1;
                if settle_ticks < 3 { return; }
                settle_ticks = 0;
                #[cfg(windows)]
                capture_title_bar(&app, "titlebar-native-maximized");
                #[cfg(windows)]
                assert!(!rounded_region(&app));
                app.invoke_maximize_window();
                stage = 3;
            }
            3 if !app.get_window_maximized() => {
                settle_ticks += 1;
                if settle_ticks < 3 { return; }
                settle_ticks = 0;
                #[cfg(windows)]
                capture_title_bar(&app, "titlebar-native-restored");
                #[cfg(windows)]
                assert!(rounded_region(&app));
                println!("Native avatar menu / rounded window / maximize-restore: true");
                native_passed.set(true);
                app.set_dark_mode(false);
                stage = 4;
            }
            4 => {
                settle_ticks += 1;
                if settle_ticks < 3 { return; }
                #[cfg(windows)]
                capture_title_bar(&app, "titlebar-native-light");
                app.invoke_show_account_menu();
                stage = 5;
            }
            _ => {}
        }
        if stage == 5 && browser_passed.get() {
            let _ = slint::quit_event_loop();
        }
        if !sent && stage == 5 {
            sent = host.evaluate_when_ready(r#"
                window.educoder.receive({type:'task', task:{game_identifier:'native-demo',title:'Native preview',description_markdown:'## Markdown ready\n<script>window.hacked=true</script>',can_submit:true},files:[{path:'main.rs',content:'fn main() {}',read_only:false}],drafts:[],samples:[],evaluation:{status:0,tests:[]}});
                document.dispatchEvent(new PointerEvent('pointerdown'));
                (async()=>{
                    const wait=async predicate=>{for(let i=0;i<100;i++){if(predicate())return;await new Promise(r=>setTimeout(r,50));}throw Error('Native answers timed out');};
                    document.querySelector('#answers').click();
                    await wait(()=>document.querySelector('#answer-code').value.includes('native answer'));
                    document.querySelector('#copy-answer').click();
                    await wait(()=>document.querySelector('#answer-status').textContent.includes('已复制'));
                    document.querySelector('#import-answer').click();
                    await wait(()=>!document.querySelector('#answer-dialog').open);
                    window.ipc.postMessage(JSON.stringify({action:'native-self-test',payload:{ok:window.educoder.editor.getValue().includes('native answer') && window.educoder.editor.getModel().getLanguageId()==='rust' && document.querySelector('#description h2')?.textContent==='Markdown ready' && !window.hacked && !document.querySelector('#description script')}}));
                })().catch(e=>window.ipc.postMessage(JSON.stringify({action:'native-self-test',payload:{ok:false,error:e.message}})));
            "#);
        }
    });
    let timeout = Timer::default();
    timeout.start(TimerMode::SingleShot, Duration::from_secs(25), || {
        eprintln!("Native workspace timed out");
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop()?;
    if !passed.get() || !chrome_passed.get() {
        return Err("native workspace verification failed".into());
    }
    Ok(())
}

#[cfg(windows)]
fn rounded_region(app: &AppWindow) -> bool {
    use slint::winit_030::{
        WinitWindowAccessor,
        winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
    };
    use windows_sys::Win32::Graphics::Gdi::{
        CreateRectRgn, DeleteObject, GetWindowRgn, PtInRegion,
    };
    app.window()
        .with_winit_window(|window| {
            let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
                return false;
            };
            unsafe {
                let region = CreateRectRgn(0, 0, 0, 0);
                let kind = GetWindowRgn(handle.hwnd.get() as *mut _, region);
                let rounded =
                    kind != 0 && PtInRegion(region, 0, 0) == 0 && PtInRegion(region, 40, 40) != 0;
                DeleteObject(region);
                rounded
            }
        })
        .unwrap_or(false)
}

/// Capture only this synthetic window's actual native title bar, including NC paint.
#[cfg(windows)]
fn capture_title_bar(app: &AppWindow, name: &str) {
    use slint::winit_030::{
        WinitWindowAccessor,
        winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    app.window().with_winit_window(|window| {
        let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as *mut _;
        let width = window.inner_size().width;
        let height = (46.0 * window.scale_factor()).ceil() as u32;
        println!(
            "Native chrome: decorated={}; scale={}",
            window.is_decorated(),
            window.scale_factor()
        );
        unsafe {
            assert!(!window.is_decorated());
            assert_eq!(
                GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION,
                0,
                "native caption style was restored"
            );
            SendMessageW(hwnd, WM_NCPAINT, 1, 0);
            SendMessageW(hwnd, WM_NCACTIVATE, 0, 0);
            SendMessageW(hwnd, WM_NCACTIVATE, 1, 0);
            let pixels = native_capture::pixels(window, height);
            std::fs::create_dir_all("target/ui-previews").unwrap();
            image::save_buffer(
                format!("target/ui-previews/{name}.png"),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            // A classic Windows caption previously painted blue over this
            // otherwise empty part of the custom bar. Inspect actual HWND pixels,
            // because Slint-only snapshots cannot detect non-client overpainting.
            let expected = if app.get_dark_mode() {
                [21, 21, 21]
            } else {
                [247, 247, 247]
            };
            let y = (18.0 * window.scale_factor()) as usize;
            let start = (200.0 * window.scale_factor()) as usize;
            let end = width as usize - (300.0 * window.scale_factor()) as usize;
            for x in start..end {
                let offset = (y * width as usize + x) * 4;
                assert_eq!(
                    &pixels[offset..offset + 3],
                    &expected,
                    "native caption covers custom title bar: {name}"
                );
            }
            let icon_x = width as usize - (72.0 * window.scale_factor()) as usize;
            let icon_y = (17.0 * window.scale_factor()) as usize;
            let icon_size = (12.0 * window.scale_factor()) as usize;
            assert!(
                (icon_y..icon_y + icon_size).any(|y| (icon_x..icon_x + icon_size).any(|x| {
                    let offset = (y * width as usize + x) * 4;
                    if app.get_dark_mode() {
                        pixels[offset] > 180 && pixels[offset + 1] > 180 && pixels[offset + 2] > 180
                    } else {
                        pixels[offset] < 100 && pixels[offset + 1] < 100 && pixels[offset + 2] < 100
                    }
                })),
                "maximize/restore icon was not painted: {name}"
            );
        }
    });
}
