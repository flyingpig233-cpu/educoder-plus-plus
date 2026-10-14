use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use educoder_core::{
    EducoderClient, Error, Result, Session, SessionVault,
    models::{Course, Homework, HomeworkKind, Page, PageRequest, User},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{AppWindow, CourseRow, HomeworkRow};

const PAGE_SIZE: u32 = 20;

#[derive(Default)]
struct State {
    client: Option<EducoderClient>,
    courses: Vec<Course>,
    course_id: Option<u64>,
    page: u32,
    kind: i32,
}

/// Only the current request may publish results after a course/type switch.
#[derive(Default)]
struct RequestVersion(AtomicU64);
impl RequestVersion {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
    fn is_current(&self, value: u64) -> bool {
        self.0.load(Ordering::SeqCst) == value
    }
}

/// UI orchestration only: the core owns all protocol and credential operations.
pub struct Controller {
    ui: slint::Weak<AppWindow>,
    runtime: Handle,
    state: Mutex<State>,
    version: RequestVersion,
    vault: SessionVault,
    cleanup_pending: AtomicBool,
}

impl Controller {
    pub fn bind(app: &AppWindow, runtime: Handle) -> Arc<Self> {
        let controller = Arc::new(Self {
            ui: app.as_weak(),
            runtime,
            state: Mutex::new(State::default()),
            version: RequestVersion::default(),
            vault: SessionVault::new("default"),
            cleanup_pending: AtomicBool::new(false),
        });
        let c = controller.clone();
        app.on_import_session(move |path| c.import(path.to_string()));
        let c = controller.clone();
        app.on_select_course(move |id| {
            if let Ok(id) = id.parse::<u64>() {
                c.select_course(id);
            }
        });
        let c = controller.clone();
        app.on_search_courses(move |query| {
            if let Some(app) = c.ui.upgrade() {
                let state = c.state.lock().expect("UI state");
                app.set_courses(course_rows(&state.courses, &query));
            }
        });
        let c = controller.clone();
        app.on_change_kind(move |kind| {
            if !(0..=1).contains(&kind) {
                return;
            }
            {
                let mut state = c.state.lock().expect("UI state");
                state.kind = kind;
                state.page = 1;
            }
            c.load_homeworks();
        });
        let c = controller.clone();
        app.on_change_page(move |direction| {
            let Some(app) = c.ui.upgrade() else {
                return;
            };
            if app.get_busy() {
                return;
            }
            let permitted = (direction == -1 && app.get_can_previous())
                || (direction == 1 && app.get_can_next());
            if !permitted {
                return;
            }
            {
                let mut state = c.state.lock().expect("UI state");
                state.page = if direction == 1 {
                    state.page.saturating_add(1)
                } else {
                    state.page.saturating_sub(1).max(1)
                };
            }
            c.load_homeworks();
        });
        let c = controller.clone();
        app.on_refresh(move || c.refresh());
        let c = controller.clone();
        app.on_refresh_courses(move || c.refresh_courses());
        let c = controller.clone();
        app.on_retry(move || {
            if c.cleanup_pending.load(Ordering::SeqCst) {
                c.logout();
            } else if c.state.lock().expect("UI state").client.is_some() {
                c.refresh();
            } else {
                c.restore();
            }
        });
        let c = controller.clone();
        app.on_logout(move || c.logout());
        controller
    }

    pub fn restore(self: &Arc<Self>) {
        let vault = self.vault.clone();
        self.spawn(
            async move {
                let session = if let Some(path) = std::env::var_os("EDUCODER_COOKIES") {
                    Some(Session::from_file(path).await?)
                } else {
                    vault.load().await?
                };
                match session {
                    Some(session) => authenticate(session).await.map(Some),
                    None => Ok(None),
                }
            },
            |c, app, result| match result {
                Ok(Some(login)) => c.connected(app, login, String::new()),
                Ok(None) => {
                    app.set_notice("连接头歌账号后，即可查看课堂作业。".into());
                }
                Err(error) => c.fail(app, error),
            },
        );
    }

    fn import(self: &Arc<Self>, path: String) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        let path = path.trim().trim_matches('"').to_owned();
        let vault = self.vault.clone();
        self.spawn(
            async move {
                let session = Session::from_file(path).await?;
                let login = authenticate(session.clone()).await?;
                let warning = if vault.save(&session).await.is_err() {
                    "已连接，但系统凭据库未能保存会话；下次启动需要重新导入。".to_owned()
                } else {
                    String::new()
                };
                Ok::<_, Error>((login, warning))
            },
            |c, app, result| match result {
                Ok((login, warning)) => c.connected(app, login, warning),
                Err(error) => c.fail(app, error),
            },
        );
    }

    fn connected(self: &Arc<Self>, app: &AppWindow, login: Login, mut notice: String) {
        self.cleanup_pending.store(false, Ordering::SeqCst);
        if login
            .courses
            .total
            .is_some_and(|n| n > login.courses.items.len() as u64)
        {
            if !notice.is_empty() {
                notice.push(' ');
            }
            notice.push_str("当前显示平台接口返回的课堂，可能未包含全部课堂。");
        }
        let first_id = login.courses.items.first().map(|c| c.id);
        app.set_connected(true);
        app.set_user_name(login.user.display_name.into());
        app.set_login_open(false);
        app.set_cookie_path("".into());
        app.set_course_query("".into());
        app.set_notice(notice.into());
        app.set_courses(course_rows(&login.courses.items, ""));
        app.set_homeworks(ModelRc::default());
        app.set_has_course(false);
        app.set_course_title("课堂作业".into());
        app.set_selected_course_id("".into());
        app.set_kind(0);
        *self.state.lock().expect("UI state") = State {
            client: Some(login.client),
            courses: login.courses.items,
            course_id: None,
            page: 1,
            kind: 0,
        };
        if let Some(id) = first_id {
            self.select_course(id);
        }
    }

    fn select_course(self: &Arc<Self>, id: u64) {
        {
            let mut state = self.state.lock().expect("UI state");
            if !state.courses.iter().any(|c| c.id == id) {
                return;
            }
            state.course_id = Some(id);
            state.page = 1;
        }
        self.load_homeworks();
    }

    fn load_homeworks(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        let (client, id, page, kind, title) = {
            let state = self.state.lock().expect("UI state");
            let (Some(client), Some(id)) = (&state.client, state.course_id) else {
                return;
            };
            (
                client.clone(),
                id,
                state.page.max(1),
                state.kind,
                state
                    .courses
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
            )
        };
        app.set_has_course(true);
        app.set_course_title(title.into());
        app.set_selected_course_id(id.to_string().into());
        app.set_kind(kind);
        app.set_homeworks(ModelRc::default());
        app.set_page_label(format!("第 {page} 页").into());
        app.set_can_previous(page > 1);
        app.set_can_next(false);
        self.spawn(
            async move {
                client
                    .get_homeworks(
                        id,
                        if kind == 0 {
                            HomeworkKind::Shixun
                        } else {
                            HomeworkKind::Text
                        },
                        PageRequest {
                            page,
                            limit: PAGE_SIZE,
                        },
                    )
                    .await
            },
            move |c, app, result| match result {
                Ok(response) => {
                    let count = response.items.len();
                    app.set_can_next(has_next_page(page, count, response.total));
                    app.set_page_label(
                        match response.total {
                            Some(total) => format!("第 {page} 页 · 共 {total} 项"),
                            None => format!("第 {page} 页 · 本页 {count} 项"),
                        }
                        .into(),
                    );
                    app.set_homeworks(homework_rows(response.items, kind));
                }
                Err(error) => c.fail(app, error),
            },
        );
    }

    fn refresh(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        let selected = {
            let state = self.state.lock().expect("UI state");
            state.course_id
        };
        if selected.is_some() {
            self.load_homeworks();
            return;
        }
        self.refresh_courses();
    }

    fn refresh_courses(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        let client = self.state.lock().expect("UI state").client.clone();
        if let Some(client) = client {
            self.spawn(
                async move {
                    let user = client.get_user_info().await?;
                    let courses = client.get_courses().await?;
                    Ok::<_, Error>(Login {
                        client,
                        user,
                        courses,
                    })
                },
                |c, app, result| match result {
                    Ok(login) => c.connected(app, login, String::new()),
                    Err(error) => c.fail(app, error),
                },
            );
        }
    }

    fn logout(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        let client = self.state.lock().expect("UI state").client.clone();
        self.reset(&app);
        let vault = self.vault.clone();
        self.spawn(
            async move {
                if let Some(client) = client {
                    client.logout(Some(&vault)).await
                } else {
                    vault.clear().await
                }
            },
            |c, app, result| {
                c.cleanup_pending.store(result.is_err(), Ordering::SeqCst);
                if result.is_err() {
                    app.set_error_message(
                        "当前会话已断开，但系统凭据库清理失败，请重试退出登录。".into(),
                    );
                } else {
                    app.set_notice("已退出登录。".into());
                }
            },
        );
    }

    fn reset(&self, app: &AppWindow) {
        *self.state.lock().expect("UI state") = State::default();
        app.set_connected(false);
        app.set_has_course(false);
        app.set_courses(ModelRc::default());
        app.set_homeworks(ModelRc::default());
        app.set_selected_course_id("".into());
        app.set_course_query("".into());
        app.set_course_title("课堂作业".into());
        app.set_user_name("尚未连接头歌".into());
        app.set_notice("".into());
        app.set_error_message("".into());
        app.set_can_next(false);
        app.set_can_previous(false);
    }

    fn fail(&self, app: &AppWindow, error: Error) {
        if matches!(error, Error::SessionExpired) {
            self.reset(app);
            app.set_login_open(true);
        }
        app.set_error_message(error.to_string().into());
    }

    fn spawn<T, F, A>(self: &Arc<Self>, future: F, apply: A)
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
        A: FnOnce(&Arc<Self>, &AppWindow, T) + Send + 'static,
    {
        let token = self.version.next();
        if let Some(app) = self.ui.upgrade() {
            app.set_busy(true);
            app.set_error_message("".into());
        }
        let controller = self.clone();
        self.runtime.spawn(async move {
            let result = future.await;
            let ui = controller.ui.clone();
            let _ = ui.upgrade_in_event_loop(move |app| {
                if !controller.version.is_current(token) {
                    return;
                }
                app.set_busy(false);
                apply(&controller, &app, result);
            });
        });
    }
}

struct Login {
    client: EducoderClient,
    user: User,
    courses: Page<Course>,
}

async fn authenticate(session: Session) -> Result<Login> {
    let client = EducoderClient::new(session)?;
    let user = client.validate_session().await?;
    let courses = client.get_courses().await?;
    Ok(Login {
        client,
        user,
        courses,
    })
}

fn course_rows(courses: &[Course], query: &str) -> ModelRc<CourseRow> {
    let query = query.trim().to_lowercase();
    let rows = courses
        .iter()
        .filter(|c| c.name.to_lowercase().contains(&query))
        .map(|c| CourseRow {
            id: c.id.to_string().into(),
            title: c.name.clone().into(),
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn homework_rows(items: Vec<Homework>, kind: i32) -> ModelRc<HomeworkRow> {
    let rows = items
        .into_iter()
        .map(|h| HomeworkRow {
            title: h.name.into(),
            subtitle: format!(
                "作业 #{}{}",
                h.id,
                h.shixun_identifier
                    .map(|id| format!("  ·  实训 {id}"))
                    .unwrap_or_default()
            )
            .into(),
            // Instance existence is not a submission/completion status.
            badge: if kind == 0 {
                "实训作业"
            } else {
                "图文作业"
            }
            .into(),
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn has_next_page(page: u32, count: usize, total: Option<u64>) -> bool {
    count > 0
        && total
            .map(|total| u64::from(page) * u64::from(PAGE_SIZE) < total)
            .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn async_ui_updates_ignore_old_results_and_expiry_clears_account() {
        use slint::platform::{
            EventLoopProxy, Platform, WindowAdapter,
            software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        };
        use std::{rc::Rc, sync::mpsc, time::Duration};

        type UiEvent = Box<dyn FnOnce() + Send>;
        struct Proxy(mpsc::Sender<UiEvent>);
        impl EventLoopProxy for Proxy {
            fn quit_event_loop(&self) -> std::result::Result<(), slint::EventLoopError> {
                Ok(())
            }
            fn invoke_from_event_loop(
                &self,
                event: UiEvent,
            ) -> std::result::Result<(), slint::EventLoopError> {
                self.0
                    .send(event)
                    .map_err(|_| slint::EventLoopError::EventLoopTerminated)
            }
        }
        struct TestPlatform(mpsc::Sender<UiEvent>);
        impl Platform for TestPlatform {
            fn create_window_adapter(
                &self,
            ) -> std::result::Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
            }
            fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
                Some(Box::new(Proxy(self.0.clone())))
            }
        }
        let (sender, events) = mpsc::channel();
        slint::platform::set_platform(Box::new(TestPlatform(sender))).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let app = AppWindow::new().unwrap();
        let controller = Controller::bind(&app, runtime.handle().clone());
        let (release, delayed) = tokio::sync::oneshot::channel();
        controller.spawn(
            async move { delayed.await.unwrap() },
            |_, app, title: String| app.set_course_title(title.into()),
        );
        controller.spawn(async { "新课堂".to_owned() }, |_, app, title| {
            app.set_course_title(title.into())
        });
        assert!(app.get_busy());
        events.recv_timeout(Duration::from_secs(5)).unwrap()();
        assert_eq!(app.get_course_title(), "新课堂");
        assert!(!app.get_busy());
        release.send("旧课堂".to_owned()).unwrap();
        events.recv_timeout(Duration::from_secs(5)).unwrap()();
        assert_eq!(app.get_course_title(), "新课堂");
        app.set_connected(true);
        app.set_user_name("原用户".into());
        app.set_has_course(true);
        controller.fail(&app, Error::SessionExpired);
        assert!(!app.get_connected());
        assert!(!app.get_has_course());
        assert!(app.get_login_open());
        assert_eq!(app.get_user_name(), "尚未连接头歌");
        assert!(app.get_error_message().contains("过期"));
    }

    #[test]
    fn pagination_handles_missing_totals_and_empty_pages() {
        assert!(has_next_page(1, 20, Some(21)));
        assert!(!has_next_page(2, 1, Some(21)));
        assert!(has_next_page(1, 3, None));
        assert!(!has_next_page(2, 0, None));
        assert!(!has_next_page(2, 0, Some(100)));
    }

    #[test]
    fn course_search_preserves_remote_ids() {
        let courses = vec![
            Course {
                id: u64::MAX,
                name: "Rust 程序设计".into(),
            },
            Course {
                id: 2,
                name: "数学".into(),
            },
        ];
        let rows = course_rows(&courses, " RUST ");
        assert_eq!(rows.row_count(), 1);
        assert_eq!(rows.row_data(0).unwrap().id, u64::MAX.to_string());
        assert_eq!(course_rows(&courses, "不存在").row_count(), 0);
    }
}
