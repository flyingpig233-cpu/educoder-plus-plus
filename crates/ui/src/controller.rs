use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use educoder_core::{
    EducoderClient, Error, Result, Session, SessionVault,
    cache::CacheStore,
    models::{Course, Homework, HomeworkKind, Page, PageRequest, User},
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use tokio::runtime::Handle;

use crate::{
    AppWindow, CourseRow, HomeworkRow, preferences::Preferences, workbench::WorkbenchState,
};
use serde::{Deserialize, Serialize};

const PAGE_SIZE: u32 = 20;

#[derive(Default)]
pub(super) struct State {
    pub client: Option<EducoderClient>,
    pub cache: Option<CacheStore>,
    pub user: Option<User>,
    pub courses: Vec<Course>,
    pub course_id: Option<u64>,
    pub page: u32,
    pub kind: i32,
    pub homeworks: Vec<Homework>,
    pub homework_progress: HashMap<u64, crate::progress::HomeworkProgress>,
    pages: HashMap<(u64, i32, u32), (Instant, Page<Homework>)>,
    pub workspace: Option<educoder_core::workspace::WorkspaceCache>,
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
    pub(super) ui: slint::Weak<AppWindow>,
    pub(super) runtime: Handle,
    pub(super) state: Mutex<State>,
    version: RequestVersion,
    auth_version: RequestVersion,
    pub(super) avatar_version: AtomicU64,
    pub(super) prefetch_version: AtomicU64,
    pub(super) prefetch_job: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub(super) preferences: Mutex<Preferences>,
    pub(super) workbench: Mutex<WorkbenchState>,
    pub(super) operation: Arc<AtomicBool>,
    pub(super) web_outbox: Mutex<VecDeque<serde_json::Value>>,
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
            auth_version: RequestVersion::default(),
            avatar_version: AtomicU64::new(0),
            prefetch_version: AtomicU64::new(0),
            prefetch_job: Mutex::new(None),
            preferences: Mutex::new(Preferences::default()),
            workbench: Mutex::new(WorkbenchState::default()),
            operation: Arc::new(AtomicBool::new(false)),
            web_outbox: Mutex::new(VecDeque::new()),
            vault: SessionVault::new("default"),
            cleanup_pending: AtomicBool::new(false),
        });
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
            if app.get_busy() || c.operation.load(Ordering::SeqCst) {
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
        let c = controller.clone();
        app.on_open_homework(move |id| {
            if let Ok(id) = id.parse::<u64>() {
                c.open_homework(id);
            }
        });
        let c = controller.clone();
        app.on_web_event(move |message| c.handle_web_event(&message));
        let c = controller.clone();
        app.on_save_settings(move || c.save_preferences());
        let c = controller.clone();
        app.on_inspect_environment(move || c.inspect_environment());
        let c = controller.clone();
        app.on_close_window(move || c.request_close());
        controller
    }

    pub fn restore(self: &Arc<Self>) {
        let vault = self.vault.clone();
        self.spawn(
            async move {
                let session = vault.load().await?;
                let Some(session) = session else {
                    return Ok::<_, Error>(None);
                };
                let cache = CacheStore::new(&session.cache_namespace()?).ok();
                let profile = if let Some(store) = &cache {
                    store
                        .get::<Profile>("profile", Duration::from_secs(600))
                        .await
                        .map(|p| p.value)
                } else {
                    None
                };
                let client = EducoderClient::new(session)?;
                Ok(Some((client, cache, profile)))
            },
            |c, app, result| match result {
                Ok(Some((client, cache, profile))) => {
                    app.set_has_session(true);
                    app.set_connected(false);
                    {
                        let mut state = c.state.lock().expect("UI state");
                        state.client = Some(client);
                        state.cache = cache;
                    }
                    if let Some(profile) = profile {
                        c.apply_profile(app, profile, false);
                        app.set_cache_status("已恢复本地缓存，正在验证连接…".into());
                    }
                    c.authenticate_current();
                }
                Ok(None) => app.set_notice("连接头歌账号后，即可查看课堂作业。".into()),
                Err(error) => c.fail(app, error),
            },
        );
    }

    pub(crate) fn import_browser_session(self: &Arc<Self>, session: Session) {
        let Some(app) = self.ui.upgrade() else { return };
        if app.get_busy() || self.operation.load(Ordering::SeqCst) {
            return;
        }
        let vault = self.vault.clone();
        self.spawn(
            async move {
                let login = authenticate(session.clone()).await?;
                let warning = if vault.save(&session).await.is_err() {
                    "已连接，但系统凭据库未能保存会话；下次启动需要重新登录。".to_owned()
                } else {
                    String::new()
                };
                Ok::<_, Error>((login, warning))
            },
            |c, app, result| match result {
                Ok((login, warning)) => c.connected(app, login, warning),
                // An unsuccessful account switch must not discard the current
                // account or its drafts. Only a validated session replaces it.
                Err(error) => app.set_error_message(error.to_string().into()),
            },
        );
    }

    fn connected(self: &Arc<Self>, app: &AppWindow, login: Login, notice: String) {
        self.cancel_prefetch();
        self.cleanup_pending.store(false, Ordering::SeqCst);
        self.auth_version.next();
        self.version.next();
        self.clear_workbench(app);
        app.set_page(0);
        app.set_has_session(true);
        app.set_connected(true);
        app.set_login_open(false);
        app.set_user_avatar(slint::Image::default());
        app.invoke_hide_account_menu();
        app.set_course_query("".into());
        app.set_notice(notice.into());
        app.set_kind(0);
        *self.state.lock().expect("UI state") = State {
            client: Some(login.client),
            cache: login.cache,
            page: 1,
            ..Default::default()
        };
        self.apply_profile(
            app,
            Profile {
                user: login.user,
                courses: login.courses,
            },
            true,
        );
    }

    fn apply_profile(self: &Arc<Self>, app: &AppWindow, profile: Profile, verified: bool) {
        app.set_connected(verified);
        app.set_has_session(true);
        app.set_user_name(profile.user.display_name.clone().into());
        app.set_user_login(profile.user.login.clone().into());
        self.load_avatar(profile.user.clone(), verified);
        if profile
            .courses
            .total
            .is_some_and(|n| n > profile.courses.items.len() as u64)
        {
            app.set_notice("当前显示接口返回的课堂，可能未包含全部课堂。".into());
        }
        app.set_courses(course_rows(&profile.courses.items, &app.get_course_query()));
        let selected = {
            let mut state = self.state.lock().expect("UI state");
            state.user = Some(profile.user);
            state.courses = profile.courses.items;
            state
                .course_id
                .filter(|id| state.courses.iter().any(|c| c.id == *id))
                .or_else(|| state.courses.first().map(|c| c.id))
        };
        if let Some(id) = selected {
            self.select_course_direct(id);
        } else {
            app.set_has_course(false);
            app.set_homeworks(ModelRc::default());
        }
    }

    fn authenticate_current(self: &Arc<Self>) {
        let (client, cache) = {
            let state = self.state.lock().expect("UI state");
            (state.client.clone(), state.cache.clone())
        };
        let Some(client) = client else { return };
        let token = self.auth_version.next();
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let result = async {
                let user = client.validate_session().await?;
                let courses = client.get_courses_for(&user.login).await?;
                let profile = Profile { user, courses };
                if let Some(store) = cache {
                    let _ = store.put("profile", &profile).await;
                }
                Ok::<_, Error>(profile)
            }
            .await;
            let _ = ui.upgrade_in_event_loop(move |app| {
                if !c.auth_version.is_current(token) {
                    return;
                }
                match result {
                    Ok(profile) => {
                        c.apply_profile(&app, profile, true);
                    }
                    Err(error) => {
                        if matches!(error, Error::SessionExpired) {
                            c.fail(&app, error);
                        } else {
                            app.set_connected(false);
                            app.set_notice("暂时无法连接头歌，仍可浏览已缓存的数据。".into());
                            app.set_error_message(error.to_string().into());
                        }
                    }
                }
            });
        });
    }

    fn select_course(self: &Arc<Self>, id: u64) {
        if let Some(app) = self.ui.upgrade()
            && self.has_editor()
        {
            self.send_web(
                &app,
                serde_json::json!({"type":"navigate","target":{"course":id}}),
            );
            return;
        }
        self.select_course_direct(id);
    }

    pub(super) fn select_course_direct(self: &Arc<Self>, id: u64) {
        {
            let mut state = self.state.lock().expect("UI state");
            if !state.courses.iter().any(|c| c.id == id) {
                return;
            }
            if state.course_id != Some(id) {
                state.page = 1;
            }
            state.course_id = Some(id);
        }
        self.load_homeworks();
    }

    fn load_homeworks(self: &Arc<Self>) {
        self.load_homeworks_with_policy(false);
    }

    fn load_homeworks_with_policy(self: &Arc<Self>, force: bool) {
        let Some(app) = self.ui.upgrade() else { return };
        self.cancel_prefetch();
        let (client, cache, id, page, kind, title, memory) = {
            let state = self.state.lock().expect("UI state");
            let (Some(client), Some(id)) = (&state.client, state.course_id) else {
                return;
            };
            let page = state.page.max(1);
            (
                client.clone(),
                state.cache.clone(),
                id,
                page,
                state.kind,
                state
                    .courses
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
                state.pages.get(&(id, state.kind, page)).cloned(),
            )
        };
        let token = self.version.next();
        app.set_has_course(true);
        app.set_course_title(title.into());
        app.set_selected_course_id(id.to_string().into());
        app.set_kind(kind);
        app.set_error_message("".into());
        app.set_can_previous(page > 1);
        if let Some((loaded, value)) = memory {
            self.show_page(&app, (id, kind, page), value, false, false);
            if !force && loaded.elapsed() < Duration::from_secs(600) {
                app.set_busy(false);
                return;
            }
        } else {
            app.set_homeworks(ModelRc::default());
            app.set_can_next(false);
            app.set_page_label(format!("第 {page} 页").into());
        }
        app.set_busy(true);
        let c = self.clone();
        let ui = self.ui.clone();
        let key = format!("homeworks:{id}:{kind}:{page}");
        self.runtime.spawn(async move {
            if !force
                && let Some(store) = &cache
                && let Some(cached) = store
                    .get::<Page<Homework>>(&key, Duration::from_secs(600))
                    .await
            {
                let fresh = !cached.stale;
                let c2 = c.clone();
                let ui2 = ui.clone();
                let value = cached.value;
                let _ = ui2.upgrade_in_event_loop(move |app| {
                    if c2.version.is_current(token) {
                        c2.show_page(&app, (id, kind, page), value, false, false);
                        app.set_busy(!fresh);
                    }
                });
                if fresh {
                    return;
                }
            }
            let result = client
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
                .await;
            if let (Ok(value), Some(store)) = (&result, &cache) {
                let _ = store.put(&key, value).await;
            }
            let _ = ui.upgrade_in_event_loop(move |app| {
                if !c.version.is_current(token) {
                    return;
                }
                app.set_busy(false);
                match result {
                    Ok(value) => c.show_page(&app, (id, kind, page), value, true, force),
                    Err(error) => c.fail(&app, error),
                }
            });
        });
    }

    pub(super) fn set_homework_games(
        &self,
        app: &AppWindow,
        id: u64,
        games: &[educoder_core::models::Game],
        force: bool,
    ) {
        self.state
            .lock()
            .expect("UI state")
            .homework_progress
            .entry(id)
            .or_default()
            .set_games(games, force);
        self.publish_homework_progress(app, id);
    }

    pub(super) fn set_unstarted_progress(
        &self,
        app: &AppWindow,
        id: u64,
        total: usize,
        confirmed: bool,
    ) {
        self.state
            .lock()
            .expect("UI state")
            .homework_progress
            .entry(id)
            .or_default()
            .set_unstarted(total, confirmed);
        self.publish_homework_progress(app, id);
    }

    pub(super) fn update_homework_progress(
        &self,
        app: &AppWindow,
        id: u64,
        game: &str,
        status: i32,
    ) {
        let status = if app.get_page() == 2 {
            // A background snapshot may have started before the latest save or
            // evaluation. Prefer the currently displayed editor result for it.
            let state = self.workbench.lock().expect("workbench");
            state.homework_progress_status(id, game).unwrap_or(status)
        } else {
            status
        };
        if let Some(progress) = self
            .state
            .lock()
            .expect("UI state")
            .homework_progress
            .get_mut(&id)
        {
            progress.update(game, status);
        }
        self.publish_homework_progress(app, id);
    }

    fn publish_homework_progress(&self, app: &AppWindow, id: u64) {
        let display = {
            let state = self.state.lock().expect("UI state");
            if state.kind != 0 {
                return;
            }
            state
                .homework_progress
                .get(&id)
                .map(crate::progress::HomeworkProgress::display)
        };
        let Some((progress, completed)) = display else {
            return;
        };
        let rows = app.get_homeworks();
        let Some(model) = rows.as_any().downcast_ref::<VecModel<HomeworkRow>>() else {
            return;
        };
        let id = id.to_string();
        for (index, mut row) in model.iter().enumerate() {
            if row.id == id && (row.progress != progress || row.completed != completed) {
                row.progress = progress.clone().into();
                row.completed = completed;
                model.set_row_data(index, row);
                break;
            }
        }
    }

    fn show_page(
        self: &Arc<Self>,
        app: &AppWindow,
        key: (u64, i32, u32),
        page: Page<Homework>,
        remote: bool,
        force: bool,
    ) {
        let count = page.items.len();
        app.set_can_next(has_next_page(key.2, count, page.total));
        app.set_page_label(
            match page.total {
                Some(total) => format!("第 {} 页 · 共 {total} 项", key.2),
                None => format!("第 {} 页 · 本页 {count} 项", key.2),
            }
            .into(),
        );
        app.set_homeworks(homework_rows(
            page.items.clone(),
            key.1,
            &self.state.lock().expect("UI state").homework_progress,
        ));
        app.set_cache_status(
            if remote {
                "刚刚更新"
            } else {
                "已显示本地缓存"
            }
            .into(),
        );
        let prefetch = page.items.clone();
        let mut state = self.state.lock().expect("UI state");
        state.homeworks = page.items.clone();
        if remote {
            state.pages.insert(key, (Instant::now(), page));
        }
        drop(state);
        self.prefetch_homeworks(if key.1 == 0 { prefetch } else { Vec::new() }, force);
    }

    fn refresh(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        if app.get_busy() || self.operation.load(Ordering::SeqCst) {
            return;
        }
        let selected = {
            let state = self.state.lock().expect("UI state");
            state.course_id
        };
        if selected.is_some() {
            self.load_homeworks_with_policy(true);
            return;
        }
        self.refresh_courses();
    }

    fn refresh_courses(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else { return };
        if app.get_busy() || self.operation.load(Ordering::SeqCst) {
            return;
        }
        self.authenticate_current();
    }

    fn logout(self: &Arc<Self>) {
        if self.has_editor() {
            if let Some(app) = self.ui.upgrade() {
                self.send_web(&app, serde_json::json!({"type":"before-logout"}));
            }
            return;
        }
        self.logout_direct();
    }

    pub(super) fn logout_direct(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else {
            return;
        };
        if app.get_busy() || self.operation.load(Ordering::SeqCst) {
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
        self.cancel_prefetch();
        self.avatar_version.fetch_add(1, Ordering::SeqCst);
        app.set_user_avatar(slint::Image::default());
        app.set_user_login("".into());
        app.invoke_hide_account_menu();
        self.auth_version.next();
        self.version.next();
        self.clear_workbench(app);
        *self.state.lock().expect("UI state") = State::default();
        app.set_connected(false);
        app.set_has_session(false);
        app.set_page(0);
        app.set_cache_status("".into());
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

    pub(super) fn fail(&self, app: &AppWindow, error: Error) {
        if matches!(error, Error::SessionExpired) {
            self.reset(app);
            app.set_login_open(true);
        }
        app.set_error_message(error.to_string().into());
        self.send_web(
            app,
            serde_json::json!({"type":"error","message":error.to_string()}),
        );
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
    cache: Option<CacheStore>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Profile {
    user: User,
    courses: Page<Course>,
}

async fn authenticate(session: Session) -> Result<Login> {
    let cache = CacheStore::new(&session.cache_namespace()?).ok();
    let client = EducoderClient::new(session)?;
    let user = client.validate_session().await?;
    let courses = client.get_courses_for(&user.login).await?;
    if let Some(store) = &cache {
        let _ = store
            .put(
                "profile",
                &Profile {
                    user: user.clone(),
                    courses: courses.clone(),
                },
            )
            .await;
    }
    Ok(Login {
        client,
        user,
        courses,
        cache,
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

fn homework_rows(
    items: Vec<Homework>,
    kind: i32,
    progress: &HashMap<u64, crate::progress::HomeworkProgress>,
) -> ModelRc<HomeworkRow> {
    let rows = items
        .into_iter()
        .map(|h| {
            let (progress, completed) = if kind == 0 {
                progress
                    .get(&h.id)
                    .map(crate::progress::HomeworkProgress::display)
                    .unwrap_or(("— / —".into(), false))
            } else {
                ("— / —".into(), false)
            };
            HomeworkRow {
                id: h.id.to_string().into(),
                title: h.name.into(),
                progress: progress.into(),
                completed,
            }
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
        controller.send_web(&app, serde_json::json!({"reply_to":"first"}));
        controller.send_web(&app, serde_json::json!({"reply_to":"second"}));
        let queued = controller.drain_web_messages();
        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0]["reply_to"], "first");
        assert_eq!(queued[1]["reply_to"], "second");
        assert!(controller.drain_web_messages().is_empty());
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
        let rows = Rc::new(VecModel::from(vec![HomeworkRow {
            id: "7".into(),
            title: "示例作业".into(),
            progress: "— / —".into(),
            completed: false,
        }]));
        app.set_homeworks(ModelRc::from(rows.clone()));
        let games = ["a", "b"].map(|id| educoder_core::models::Game {
            identifier: id.into(),
            position: 1,
            name: id.into(),
        });
        controller.set_homework_games(&app, 7, &games, false);
        controller.update_homework_progress(&app, 7, "a", 2);
        assert!(!rows.row_data(0).unwrap().completed);
        controller.update_homework_progress(&app, 7, "b", 2);
        assert_eq!(rows.row_data(0).unwrap().progress, "2 / 2");
        assert!(rows.row_data(0).unwrap().completed);
        controller.update_homework_progress(&app, 7, "b", 3);
        assert!(!rows.row_data(0).unwrap().completed);
        assert!(!app.get_busy());
        release.send("旧课堂".to_owned()).unwrap();
        events.recv_timeout(Duration::from_secs(5)).unwrap()();
        assert_eq!(app.get_course_title(), "新课堂");
        app.set_connected(true);
        app.set_user_name("原用户".into());
        app.set_has_course(true);
        app.set_user_login("demo".into());
        app.set_user_avatar(slint::Image::from_rgba8(slint::SharedPixelBuffer::new(
            1, 1,
        )));
        app.invoke_show_account_menu();
        assert!(app.get_account_menu_open());
        let avatar_version = controller.avatar_version.load(Ordering::SeqCst);
        controller.fail(&app, Error::SessionExpired);
        assert!(!app.get_connected());
        assert!(!app.get_has_course());
        assert!(app.get_login_open());
        assert_eq!(app.get_user_name(), "尚未连接头歌");
        assert!(app.get_error_message().contains("过期"));
        assert!(!app.get_account_menu_open());
        assert!(app.get_user_login().is_empty());
        assert_eq!(app.get_user_avatar().size().width, 0);
        assert!(controller.avatar_version.load(Ordering::SeqCst) > avatar_version);
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
