//! Render the actual Slint component with synthetic data, without opening a window.
use std::{path::Path, rc::Rc};

use educoder_ui::{AppWindow, CourseRow, HomeworkRow};
use slint::{
    ComponentHandle, ModelRc, PhysicalSize, VecModel,
    platform::{
        Platform, WindowAdapter,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};

struct PreviewPlatform(Rc<MinimalSoftwareWindow>);
impl Platform for PreviewPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
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
    slint::platform::set_platform(Box::new(PreviewPlatform(window.clone())))?;
    let app = AppWindow::new()?;
    window.set_size(PhysicalSize::new(1160, 760));
    app.show()?;
    snapshot(&app, "welcome-dark")?;
    app.set_connected(true);
    app.set_has_session(true);
    app.set_user_name("预览账号 · 示例数据".into());
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
            subtitle: "作业 #1001  ·  实训 rust_intro".into(),
            badge: "实训作业".into(),
        },
        HomeworkRow {
            id: "1002".into(),
            title: "02 · 所有权、借用与生命周期".into(),
            subtitle: "作业 #1002  ·  实训 ownership".into(),
            badge: "实训作业".into(),
        },
        HomeworkRow {
            id: "1003".into(),
            title: "03 · 使用枚举与模式匹配".into(),
            subtitle: "作业 #1003  ·  实训 patterns".into(),
            badge: "实训作业".into(),
        },
        HomeworkRow {
            id: "1004".into(),
            title: "04 · 集合类型与错误处理：构建一个可复用的命令行工具".into(),
            subtitle: "作业 #1004  ·  实训 collections".into(),
            badge: "实训作业".into(),
        },
    ])));
    app.set_page_label("第 1 页 · 共 24 项".into());
    app.set_can_next(true);
    snapshot(&app, "homeworks-dark")?;
    app.set_dark_mode(false);
    snapshot(&app, "homeworks-light")?;
    window.set_size(PhysicalSize::new(900, 620));
    snapshot(&app, "compact-light")?;
    app.set_login_open(true);
    app.set_error_message("Cookie 文件无法读取，请检查路径后重试。".into());
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
    println!("Previews saved to target/ui-previews/");
    Ok(())
}
