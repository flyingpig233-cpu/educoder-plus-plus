//! Render the actual Slint component with synthetic data, without opening a window.
use std::{cell::Cell, path::Path, rc::Rc, time::Duration};

use educoder_ui::{AppWindow, CourseRow, HomeworkRow};
use slint::{
    ComponentHandle, ModelRc, PhysicalSize, VecModel,
    platform::{
        Platform, WindowAdapter,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};

struct PreviewPlatform(Rc<MinimalSoftwareWindow>, Rc<Cell<Duration>>);
impl Platform for PreviewPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.1.get()
    }
}

fn snapshot(app: &AppWindow, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    slint::platform::update_timers_and_animations();
    let image = app.window().take_snapshot()?;
    image::save_buffer(
        Path::new("target/ui-previews").join(format!("{name}.png")),
        image.as_bytes(),
        image.width(),
        image.height(),
        image::ColorType::Rgba8,
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("target/ui-previews")?;
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let clock = Rc::new(Cell::new(Duration::ZERO));
    slint::platform::set_platform(Box::new(PreviewPlatform(window.clone(), clock.clone())))?;
    let app = AppWindow::new()?;
    app.set_reduce_motion(true);
    window.set_size(PhysicalSize::new(1160, 760));
    app.show()?;
    snapshot(&app, "welcome-dark")?;
    app.set_dark_mode(false);
    snapshot(&app, "welcome-light")?;
    window.set_size(PhysicalSize::new(900, 620));
    snapshot(&app, "welcome-compact-light")?;
    app.invoke_toggle_sidebar();
    snapshot(&app, "welcome-collapsed-light")?;
    app.invoke_toggle_sidebar();
    app.set_busy(true);
    snapshot(&app, "welcome-loading-light")?;
    app.set_busy(false);
    app.set_dark_mode(true);
    window.set_size(PhysicalSize::new(1160, 760));
    app.set_connected(true);
    app.set_has_session(true);
    app.set_user_name("预览账号 · 示例数据".into());
    app.set_user_login("demo-student".into());
    app.set_course_title("Rust 程序设计".into());
    app.set_selected_course_id("1".into());
    app.set_has_course(true);
    app.set_courses(ModelRc::new(VecModel::from(vec![
        CourseRow {
            id: "1".into(),
            title: "Rust 程序设计".into(),
        },
        CourseRow {
            id: "2".into(),
            title: "数据结构与算法".into(),
        },
        CourseRow {
            id: "3".into(),
            title: "操作系统原理与实践".into(),
        },
        CourseRow {
            id: "4".into(),
            title: "计算机网络".into(),
        },
    ])));
    app.set_homeworks(ModelRc::new(VecModel::from(vec![
        HomeworkRow {
            id: "1001".into(),
            title: "01 · 从 Hello World 开始".into(),
            progress: "4 / 4".into(),
            completed: true,
        },
        HomeworkRow {
            id: "1002".into(),
            title: "02 · 所有权、借用与生命周期".into(),
            progress: "0 / 4".into(),
            completed: false,
        },
        HomeworkRow {
            id: "1003".into(),
            title: "03 · 使用枚举与模式匹配".into(),
            progress: "0 / 4".into(),
            completed: false,
        },
        HomeworkRow {
            id: "1004".into(),
            title: "04 · 集合类型与错误处理：构建一个可复用的命令行工具".into(),
            progress: "0 / 4".into(),
            completed: false,
        },
    ])));
    app.set_page_label("第 1 页 · 共 24 项".into());
    app.set_can_next(true);
    snapshot(&app, "homeworks-dark")?;
    app.invoke_show_account_menu();
    snapshot(&app, "account-menu-dark")?;
    app.invoke_hide_account_menu();
    app.set_dark_mode(false);
    snapshot(&app, "homeworks-light")?;
    app.invoke_show_account_menu();
    snapshot(&app, "account-menu-light")?;
    app.invoke_hide_account_menu();
    window.set_size(PhysicalSize::new(900, 620));
    snapshot(&app, "compact-light")?;
    app.set_login_open(true);
    app.set_error_message("无法读取登录会话，请完成微信扫码确认后重试。".into());
    snapshot(&app, "login-light")?;
    app.set_login_open(false);
    app.set_error_message("".into());
    app.set_busy(true);
    snapshot(&app, "loading-light")?;
    app.set_busy(false);
    app.set_page(1);
    snapshot(&app, "settings-light")?;
    app.set_dark_mode(true);
    snapshot(&app, "settings-dark")?;
    app.set_page(0);
    app.set_reduce_motion(false);
    app.invoke_show_account_menu();
    snapshot(&app, "account-menu-animation-start")?;
    clock.set(Duration::from_millis(90));
    snapshot(&app, "account-menu-animation-middle")?;
    clock.set(Duration::from_millis(200));
    snapshot(&app, "account-menu-animation-end")?;
    let start = image::open("target/ui-previews/account-menu-animation-start.png")?.to_rgba8();
    let middle = image::open("target/ui-previews/account-menu-animation-middle.png")?.to_rgba8();
    let end = image::open("target/ui-previews/account-menu-animation-end.png")?.to_rgba8();
    assert!(start != middle, "menu must animate after opening");
    assert!(middle != end, "menu must have an intermediate frame");
    app.invoke_hide_account_menu();
    snapshot(&app, "account-menu-animation-closed")?;
    app.invoke_show_account_menu();
    snapshot(&app, "account-menu-animation-reopened")?;
    assert!(
        start == image::open("target/ui-previews/account-menu-animation-reopened.png")?.to_rgba8(),
        "reopening must restart the reveal"
    );
    app.invoke_hide_account_menu();
    app.set_reduce_motion(true);
    app.invoke_show_account_menu();
    snapshot(&app, "account-menu-reduced-motion")?;
    assert!(
        end == image::open("target/ui-previews/account-menu-reduced-motion.png")?.to_rgba8(),
        "reduced motion must show the complete menu immediately"
    );
    app.invoke_hide_account_menu();
    app.set_course_query("Rust".into());
    app.set_reduce_motion(false);
    snapshot(&app, "sidebar-expanded-dark")?;
    let expanded_x = app.get_web_x();
    app.invoke_toggle_sidebar();
    snapshot(&app, "sidebar-collapse-start")?;
    clock.set(Duration::from_millis(310));
    snapshot(&app, "sidebar-collapse-middle")?;
    let middle_x = app.get_web_x();
    assert!(middle_x > 65.0 && middle_x < expanded_x);
    clock.set(Duration::from_millis(440));
    snapshot(&app, "sidebar-collapsed-dark")?;
    assert_eq!(app.get_web_x(), 65.0);
    assert_eq!(app.get_web_width(), 829.0);
    app.invoke_show_account_menu();
    clock.set(Duration::from_millis(640));
    snapshot(&app, "sidebar-collapsed-account-menu")?;
    app.invoke_toggle_sidebar();
    assert!(!app.get_account_menu_open());
    snapshot(&app, "sidebar-expand-start")?;
    clock.set(Duration::from_millis(750));
    snapshot(&app, "sidebar-expand-middle")?;
    assert!(app.get_web_x() > 65.0 && app.get_web_x() < expanded_x);
    // Reverse an unfinished transition, then verify the final rail geometry.
    app.invoke_toggle_sidebar();
    snapshot(&app, "sidebar-reverse-start")?;
    clock.set(Duration::from_millis(990));
    snapshot(&app, "sidebar-reverse-end")?;
    assert_eq!(app.get_web_x(), 65.0);
    app.set_dark_mode(false);
    snapshot(&app, "sidebar-collapsed-light")?;
    app.set_reduce_motion(true);
    app.invoke_toggle_sidebar();
    snapshot(&app, "sidebar-expanded-reduced-motion")?;
    assert_eq!(app.get_web_x(), expanded_x);
    assert_eq!(app.get_selected_course_id(), "1");
    assert_eq!(app.get_course_query(), "Rust");
    app.invoke_toggle_sidebar();
    snapshot(&app, "sidebar-collapsed-reduced-motion")?;
    assert_eq!(app.get_web_x(), 65.0);
    println!("Previews saved to target/ui-previews/");
    Ok(())
}
