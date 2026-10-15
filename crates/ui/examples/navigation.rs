//! Actual HWND comparison across repeated Slint/WebView page transitions.
//! All account, coursework and editor data are synthetic; no platform requests.
#[cfg(windows)]
#[path = "support/native_capture.rs"]
mod native_capture;
use educoder_ui::{AppWindow, Controller, CourseRow, HomeworkRow, webview::WebHost};
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::{cell::Cell, rc::Rc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    educoder_ui::chrome::initialize_backend()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let app = AppWindow::new()?;
    let _chrome = educoder_ui::chrome::WindowChrome::attach(&app);
    let controller = Controller::bind(&app, runtime.handle().clone());
    let host = Rc::new(WebHost::attach(&app, controller));
    app.set_has_session(true);
    app.set_connected(true);
    app.set_has_course(true);
    app.set_user_name("示例同学".into());
    app.set_user_login("demo-student".into());
    app.set_course_title("渲染回归课堂".into());
    app.set_courses(ModelRc::new(VecModel::from(vec![CourseRow {
        id: "demo".into(),
        title: "渲染回归课堂".into(),
    }])));
    app.set_homeworks(ModelRc::new(VecModel::from(
        (1..=3)
            .map(|i| HomeworkRow {
                id: i.to_string().into(),
                title: format!("作业 {i} · 示例代码任务").into(),
                subtitle: "合成测试数据".into(),
                badge: "实训作业".into(),
            })
            .collect::<Vec<_>>(),
    )));
    let editor_preserved = Rc::new(Cell::new(false));
    let preserved = editor_preserved.clone();
    let ipc_host = host.clone();
    let ipc_ui = app.as_weak();
    app.on_web_event(move |body| {
        let message: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if message["action"] == "navigate"
            && let Some(app) = ipc_ui.upgrade()
        {
            app.set_page(0);
            #[cfg(windows)]
            paint_while_child_is_visible(&app);
        }
        if message["action"] == "native-navigation" {
            preserved.set(message["payload"]["ok"] == true);
        }
        if message["action"] == "draft" {
            let reply = serde_json::json!({"reply_to":message["id"],"ok":true,"data":{}});
            ipc_host.evaluate_when_ready(&format!("window.educoder.receive({reply});"));
        }
    });
    app.show()?;
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let weak = app.as_weak();
    let mut ticks = 0;
    let mut stage = 0;
    let mut baseline = Vec::new();
    let mut rounds = 0;
    let timer = Timer::default();
    timer.start(TimerMode::Repeated,Duration::from_millis(100),move || {
        let Some(app)=weak.upgrade() else {return};
        ticks+=1;
        match stage {
            0 if ticks>=4 => {
                baseline=capture_page(&app,"navigation-before");
                app.set_page(2);
                ticks=0;stage=1;
            }
            1 if ticks>=4 => {
                if host.evaluate_when_ready(r#"window.educoder.receive({type:'task',task:{game_identifier:'demo',title:'示例题目',description_markdown:'# Example',can_submit:true},files:[{path:'main.rs',content:'fn main() {}',read_only:false}],drafts:[],samples:[],evaluation:{status:0,tests:[]}});window.educoder.editor.setValue('preserve this local draft');"#) {
                    ticks=0;stage=2;
                }
            }
            2 if ticks>=4 => {host.evaluate_when_ready("document.getElementById('back').click();");ticks=0;stage=3;}
            3 if ticks>=4 => {
                let actual=capture_page(&app,"navigation-after");
                compare_page(&app,&baseline,&actual);
                app.set_page(2);ticks=0;stage=4;
            }
            4 if ticks>=4 => {
                editor_preserved.set(false);
                host.evaluate_when_ready("window.ipc.postMessage(JSON.stringify({action:'native-navigation',payload:{ok:window.educoder.editor.getValue()==='preserve this local draft'}}));");
                ticks=0;stage=5;
            }
            5 if editor_preserved.get() => {
                rounds+=1;
                host.evaluate_when_ready("document.getElementById('back').click();");ticks=0;
                if rounds==3 { stage=6; } else {stage=3;}
            }
            6 if ticks>=4 => {
                compare_page(&app,&baseline,&capture_page(&app,"navigation-final"));
                result.set(true);
                println!("PASS: 3 editor/list round trips; native list pixels and editor draft preserved");
                let _=slint::quit_event_loop();
            }
            _=>{}
        }
    });
    let deadline = Timer::default();
    deadline.start(TimerMode::SingleShot, Duration::from_secs(30), || {
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop()?;
    // Release the callback's WebHost reference before destroying the parent HWND.
    app.on_web_event(|_| {});
    if !passed.get() {
        return Err("navigation rendering verification failed".into());
    }
    Ok(())
}

#[cfg(windows)]
fn capture_page(app: &AppWindow, name: &str) -> Vec<u8> {
    use slint::winit_030::WinitWindowAccessor;
    let pixels = app
        .window()
        .with_winit_window(|w| native_capture::pixels(w, w.inner_size().height))
        .unwrap();
    let size = app.window().size();
    std::fs::create_dir_all("target/ui-previews").unwrap();
    image::save_buffer(
        format!("target/ui-previews/{name}.png"),
        &pixels,
        size.width,
        size.height,
        image::ColorType::Rgba8,
    )
    .unwrap();
    pixels
}
#[cfg(not(windows))]
fn capture_page(app: &AppWindow, _: &str) -> Vec<u8> {
    app.window().take_snapshot().unwrap().as_bytes().to_vec()
}
fn compare_page(app: &AppWindow, baseline: &[u8], actual: &[u8]) {
    let size = app.window().size();
    assert_eq!(baseline.len(), actual.len());
    let scale = app.window().scale_factor();
    let mut changed = 0usize;
    let mut count = 0usize;
    for y in (64.0 * scale) as usize..size.height as usize - (8.0 * scale) as usize {
        for x in (275.0 * scale) as usize..size.width as usize - (8.0 * scale) as usize {
            let offset = (y * size.width as usize + x) * 4;
            changed +=
                usize::from((0..3).any(|c| baseline[offset + c].abs_diff(actual[offset + c]) > 12));
            count += 1;
        }
    }
    let ratio = changed as f64 / count as f64;
    println!("List rendering pixel mismatch: {:.3}%", ratio * 100.0);
    assert!(
        ratio < 0.005,
        "assignment page was not fully repainted after hiding WebView"
    );
}

#[cfg(windows)]
fn paint_while_child_is_visible(app: &AppWindow) {
    use slint::winit_030::{
        WinitWindowAccessor,
        winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
    };
    use windows_sys::Win32::Graphics::Gdi::{RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow};
    app.window().with_winit_window(|window| {
        let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
            return;
        };
        // Exercise the problematic ordering: Windows paints the parent before
        // WebHost's visibility tick hides the child and exposes the list.
        unsafe {
            RedrawWindow(
                handle.hwnd.get() as *mut _,
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_UPDATENOW,
            );
        }
    });
}
