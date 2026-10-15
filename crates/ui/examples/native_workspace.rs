//! Exercises the actual Slint + WebView2 child, offline Monaco and native IPC.
//! Synthetic data only; no session is restored and no platform is contacted.
#[cfg(windows)]
#[path = "support/native_capture.rs"]
mod native_capture;
use educoder_ui::{AppWindow, Controller, webview::WebHost};
use slint::{ComponentHandle, Timer, TimerMode, winit_030::WinitWindowAccessor};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    educoder_ui::chrome::initialize_backend()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let app = AppWindow::new()?;
    app.set_theme_mode(2);
    let _chrome = educoder_ui::chrome::WindowChrome::attach(&app);
    app.set_reduce_motion(false);
    let controller = Controller::bind(&app, runtime.handle().clone());
    let host = WebHost::attach(&app, controller);
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let celebrated = Rc::new(Cell::new(false));
    let celebration_result = celebrated.clone();
    let replies = Rc::new(RefCell::new(Vec::<serde_json::Value>::new()));
    let ipc_replies = replies.clone();
    let ipc_ui = app.as_weak();
    app.on_web_event(move |body| {
        let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if value["action"] == "native-celebration" {
            celebration_result.set(value["payload"]["visible"] == true && value["payload"]["painted"] == true);
        }
        if value["action"] == "answers" || value["action"] == "draft" {
            let data = if value["action"] == "answers" {
                serde_json::json!({"answer":{"title":"Native preview","status":"public_samples_passed","editable_paths":["main.rs"],"files":[{"path":"answer/demo.rs","content":"fn main() { /* native answer */ }"}]},"cached":false,"stale":false})
            } else { serde_json::json!({}) };
            ipc_replies.borrow_mut().push(serde_json::json!({"reply_to":value["id"],"ok":true,"data":data}));
        }
        if value["action"] == "native-self-test" {
            println!("Synthetic native details: {}",value["payload"]);
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
    let mut restored_bounds = None;
    let mut transition_bounds = None;
    tick.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(app) = weak.upgrade() else { return };
        for reply in replies.borrow_mut().drain(..) {
            host.evaluate_when_ready(&format!("window.educoder.receive({reply})"));
        }
        if app.get_window_transitioning() {
            app.window().with_winit_window(|window| {
                let bounds = (window.outer_position().unwrap(), window.inner_size());
                assert_eq!(Some(bounds), transition_bounds, "compositor animation must not resize live content each frame");
            });
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
                app.window().with_winit_window(|window| {
                    restored_bounds = Some((window.outer_position().unwrap(), window.inner_size()));
                });
                app.invoke_maximize_window();
                app.window().with_winit_window(|window| {
                    transition_bounds = Some((window.outer_position().unwrap(), window.inner_size()));
                });
                stage = 2;
            }
            2 if app.get_window_maximized() && !app.get_window_transitioning() => {
                settle_ticks += 1;
                if settle_ticks < 3 { return; }
                settle_ticks = 0;
                #[cfg(windows)]
                capture_title_bar(&app, "titlebar-native-maximized");
                #[cfg(windows)]
                assert!(!rounded_region(&app));
                app.invoke_maximize_window();
                app.window().with_winit_window(|window| {
                    transition_bounds = Some((window.outer_position().unwrap(), window.inner_size()));
                });
                stage = 3;
            }
            3 if !app.get_window_maximized() && !app.get_window_transitioning() => {
                settle_ticks += 1;
                if settle_ticks < 3 { return; }
                settle_ticks = 0;
                #[cfg(windows)]
                capture_title_bar(&app, "titlebar-native-restored");
                #[cfg(windows)]
                assert!(rounded_region(&app));
                app.window().with_winit_window(|window| {
                    assert_eq!(Some((window.outer_position().unwrap(), window.inner_size())), restored_bounds, "animation lost the original restore position or size");
                });
                app.set_reduce_motion(true);
                app.invoke_maximize_window();
                assert!(!app.get_window_transitioning());
                app.window().with_winit_window(|window| assert!(window.is_maximized()));
                app.invoke_maximize_window();
                app.window().with_winit_window(|window| {
                    assert!(!window.is_maximized());
                    assert_eq!(Some((window.outer_position().unwrap(), window.inner_size())), restored_bounds);
                });
                app.set_reduce_motion(false);
                println!("Native avatar menu / rounded window / maximize-restore: true");
                native_passed.set(true);
                app.set_theme_mode(1);app.set_dark_mode(false);
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
            app.set_reduce_motion(false);app.set_celebration_game("native-demo".into());app.set_celebration_active(true);app.set_celebration_id(1);settle_ticks=0;stage=6;
        }
        if stage==6 {
            settle_ticks+=1;
            if settle_ticks==6 {
                host.evaluate_when_ready(r#"
                    const ac=document.querySelector('#ac-celebration'),c=document.querySelector('#confetti-canvas');
                    const data=c.getContext('2d').getImageData(0,0,c.width,c.height).data;
                    let painted=false;for(let i=3;i<data.length;i+=4){if(data[i]){painted=true;break;}}
                    window.ipc.postMessage(JSON.stringify({action:'native-celebration',payload:{visible:!ac.hidden,painted}}));
                "#);
            }
            if settle_ticks>=9 {
                assert!(celebrated.get(),"WebView2 celebration must be visible and paint particles");
                #[cfg(windows)]
                app.window().with_winit_window(|window| {
                    // GDI captures native chrome; WebView2 composition is checked above.
                    let size=window.inner_size();let height=(46.0*window.scale_factor()) as u32;
                    let pixels=native_capture::pixels(window,height);
                    image::save_buffer("target/ui-previews/accepted-native-chrome.png",&pixels,size.width,height,image::ColorType::Rgba8).unwrap();
                });
                app.set_theme_mode(0);stage=7;settle_ticks=0;
            }
        }
        if stage==7 {
            settle_ticks+=1;
            if settle_ticks>=3 {
                app.window().with_winit_window(|window| {if let Some(theme)=window.theme(){assert_eq!(app.get_dark_mode(),theme==slint::winit_030::winit::window::Theme::Dark);}});
                println!("Native whole-window celebration / follow system theme: true");
                app.invoke_hide_account_menu();
                app.set_reduce_motion(true);
                app.invoke_toggle_sidebar();
                app.invoke_show_account_menu();
                stage=8;settle_ticks=0;
            }
        }
        if stage==8 {
            settle_ticks+=1;
            if settle_ticks>=4 {
                #[cfg(windows)]
                app.window().with_winit_window(|window| {
                    let size=window.inner_size();
                    let pixels=native_capture::pixels(window,size.height);
                    let x=(236.0*window.scale_factor()) as usize;
                    let y=((size.height as f64/window.scale_factor()-200.0)*window.scale_factor()) as usize;
                    let offset=(y*size.width as usize+x)*4;
                    let surface=if app.get_dark_mode(){36}else{245};
                    assert_eq!(&pixels[offset..offset+3], &[surface;3], "collapsed avatar menu must paint above the native editor");
                    image::save_buffer("target/ui-previews/collapsed-menu-native.png",&pixels,size.width,size.height,image::ColorType::Rgba8).unwrap();
                });
                app.invoke_hide_account_menu();
                println!("Native collapsed avatar menu above editor: true");
                let _=slint::quit_event_loop();
            }
        }
        if !sent && stage == 5 {
            sent = host.evaluate_when_ready(r#"
                window.educoder.receive({type:'task', task:{game_identifier:'native-demo',title:'Native preview',description_markdown:'##Markdown ready\n<script>window.hacked=true</script>',can_submit:true},files:[{path:'main.rs',content:'fn main() {}',read_only:false}],drafts:[],samples:[],evaluation:{status:0,tests:[]}});
                document.dispatchEvent(new PointerEvent('pointerdown'));
                (async()=>{
                    const wait=async(predicate,label)=>{for(let i=0;i<100;i++){if(predicate())return;await new Promise(r=>setTimeout(r,50));}throw Error('Native timed out: '+label);};
                    document.querySelector('#answers').click();
                    await wait(()=>document.querySelector('#answer-code').value.includes('native answer'),'answer loaded');
                    document.querySelector('#copy-answer').click();
                    await wait(()=>document.querySelector('#answer-status').textContent.includes('已复制'),'copied');
                    document.querySelector('#import-answer').click();
                    await wait(()=>!document.querySelector('#answer-dialog').open,'imported');
                    const context=window.educoder.editor.getContribution('editor.contrib.contextmenu');
                    const actions=context._getMenuActions(window.educoder.editor.getModel(),window.educoder.editor.contextMenuId);
                    let menuReady=actions.some(a=>a.label==='Copy')&&actions.some(a=>a.label==='Paste');
                    // Windows prevents background previews from retaining popup focus.
                    // Physical menu opening is covered by the trusted Playwright click.
                    window.ipc.postMessage(JSON.stringify({action:'native-self-test',payload:{ok:menuReady && window.educoder.editor.getValue().includes('native answer') && window.educoder.editor.getModel().getLanguageId()==='rust' && document.querySelector('#description h2')?.textContent==='Markdown ready' && !window.hacked && !document.querySelector('#description script')}}));
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
/// Call only after DWM has settled: GDI does not expose the compositor's
/// transformed animation surface and cannot validate an in-flight DWM frame.
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
            if !app.get_window_transitioning() {
                assert_eq!(
                    GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION,
                    0,
                    "native caption must be removed after the compositor transition"
                );
            }
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
