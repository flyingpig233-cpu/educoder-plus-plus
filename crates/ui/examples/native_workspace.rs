//! Exercises the actual Slint + WebView2 child, offline Monaco and native IPC.
//! Synthetic data only; no session is restored and no platform is contacted.
use educoder_ui::{AppWindow, Controller, webview::WebHost};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::Cell, rc::Rc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let app = AppWindow::new()?;
    let _chrome = educoder_ui::chrome::WindowChrome::attach(&app);
    let controller = Controller::bind(&app, runtime.handle().clone());
    let host = WebHost::attach(&app, controller);
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let ipc_ui = app.as_weak();
    app.on_web_event(move |body| {
        let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if value["action"] == "native-self-test" {
            result.set(
                value["payload"]["ok"] == true
                    && ipc_ui
                        .upgrade()
                        .is_some_and(|app| !app.get_account_menu_open()),
            );
            println!("Native offline Monaco / Markdown / IPC: {}", result.get());
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
    tick.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(app) = weak.upgrade() else { return };
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
                assert!(rounded_region(&app));
                app.invoke_maximize_window();
                stage = 2;
            }
            2 if app.get_window_maximized() => {
                #[cfg(windows)]
                assert!(!rounded_region(&app));
                app.invoke_maximize_window();
                stage = 3;
            }
            3 if !app.get_window_maximized() => {
                #[cfg(windows)]
                assert!(rounded_region(&app));
                println!("Native avatar menu / rounded window / maximize-restore: true");
                native_passed.set(true);
                app.invoke_show_account_menu();
                stage = 4;
            }
            _ => {}
        }
        if stage == 4 && browser_passed.get() {
            let _ = slint::quit_event_loop();
        }
        if !sent && stage == 4 {
            sent = host.evaluate_when_ready(r#"
                window.educoder.receive({type:'task', task:{game_identifier:'native-demo',title:'Native preview',description_markdown:'## Markdown ready\n<script>window.hacked=true</script>',can_submit:true},files:[{path:'main.rs',content:'fn main() {}',read_only:false}],drafts:[],samples:[],evaluation:{status:0,tests:[]}});
                document.dispatchEvent(new PointerEvent('pointerdown'));
                setTimeout(()=>window.ipc.postMessage(JSON.stringify({action:'native-self-test',payload:{ok:window.educoder.editor.getModel().getLanguageId()==='rust' && document.querySelector('#description h2')?.textContent==='Markdown ready' && !window.hacked && !document.querySelector('#description script')}})),1000);
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
