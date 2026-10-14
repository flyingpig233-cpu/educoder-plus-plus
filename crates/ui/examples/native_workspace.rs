//! Exercises the actual Slint + WebView2 child, offline Monaco and native IPC.
//! Synthetic data only; no session is restored and no platform is contacted.
use educoder_ui::{AppWindow, Controller, webview::WebHost};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::Cell, rc::Rc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let app = AppWindow::new()?;
    let controller = Controller::bind(&app, runtime.handle().clone());
    let host = WebHost::attach(&app, controller);
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    app.on_web_event(move |body| {
        let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        if value["action"] == "native-self-test" {
            result.set(value["payload"]["ok"] == true);
            println!("Native offline Monaco / Markdown / IPC: {}", result.get());
            let _ = slint::quit_event_loop();
        }
    });
    app.set_page(2);
    app.show()?;
    let tick = Timer::default();
    let mut sent = false;
    tick.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        if !sent {
            sent = host.evaluate_when_ready(r#"
                window.educoder.receive({type:'task', task:{game_identifier:'native-demo',title:'Native preview',description_markdown:'## Markdown ready\n<script>window.hacked=true</script>',can_submit:true},files:[{path:'main.rs',content:'fn main() {}',read_only:false}],drafts:[],samples:[],evaluation:{status:0,tests:[]}});
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
    if !passed.get() {
        return Err("native workspace verification failed".into());
    }
    Ok(())
}
