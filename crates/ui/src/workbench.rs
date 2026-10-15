use crate::{AppWindow, Controller};
use educoder_core::{
    EducoderClient, Error, Result,
    models::{CodeFile, Game, Homework, TaskDetails, TaskDocument as Document},
    runner::{Sample, run_samples},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

#[derive(Default)]
pub(super) struct WorkbenchState {
    homework: Option<Homework>,
    games: Vec<Game>,
    document: Option<Document>,
    version: u64,
    ready: bool,
    pending_write: bool,
}

impl WorkbenchState {
    pub(super) fn homework_progress_status(&self, homework: u64, game: &str) -> Option<i32> {
        let document = self.document.as_ref()?;
        (self.homework.as_ref()?.id == homework && document.details.task.game_identifier == game)
            .then_some(document.details.evaluation.status)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct TextFile {
    path: String,
    content: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Draft {
    files: Vec<TextFile>,
    baseline: Vec<TextFile>,
    samples: Vec<Sample>,
    #[serde(default)]
    conflict: bool,
}
struct DisplayOptions {
    cached: bool,
    pending: bool,
    force: bool,
}
#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<String>,
    action: String,
    #[serde(default)]
    context: String,
    #[serde(default)]
    payload: Value,
}

pub(super) fn publish(app: &AppWindow, value: Value) {
    app.set_web_message(value.to_string().into());
    app.set_web_sequence(app.get_web_sequence().wrapping_add(1));
}

impl Controller {
    fn workspace_cache(&self) -> Option<educoder_core::workspace::WorkspaceCache> {
        let mut state = self.state.lock().expect("UI state");
        if state.workspace.is_none() {
            state.workspace = Some(educoder_core::workspace::WorkspaceCache::new(
                state.client.clone()?,
                state.cache.clone()?,
            ));
        }
        state.workspace.clone()
    }

    pub(super) fn cancel_prefetch(&self) -> u64 {
        let version = self.prefetch_version.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(job) = self.prefetch_job.lock().expect("prefetch job").take() {
            job.abort();
        }
        version
    }

    pub(super) fn prefetch_homeworks(self: &Arc<Self>, homeworks: Vec<Homework>, force: bool) {
        let version = self.cancel_prefetch();
        if homeworks.is_empty() {
            return;
        }
        let Some(workspace) = self.workspace_cache() else {
            return;
        };
        let (course, current_page, client, cache) = {
            let state = self.state.lock().expect("UI state");
            (
                state.course_id,
                state.page.max(1),
                state.client.clone(),
                state.cache.clone(),
            )
        };
        let c = self.clone();
        let ui = self.ui.clone();
        let job = self.runtime.spawn(async move {
            // Restore every visible row from disk before issuing any network
            // request. A slow first homework must not delay cached green rows.
            if let Some(store) = &cache {
                for homework in &homeworks {
                    let Some(cached) = store
                        .get::<Vec<Game>>(&format!("games:{}", homework.id), Duration::MAX)
                        .await
                    else {
                        continue;
                    };
                    let games = cached.value;
                    if games.is_empty() {
                        continue;
                    }
                    let mut statuses = Vec::new();
                    for game in &games {
                        let details = if let Some(cached) = store
                            .get::<TaskDetails>(
                                &format!("task-details:{}:{}", homework.id, game.identifier),
                                Duration::MAX,
                            )
                            .await
                        {
                            Some(cached.value)
                        } else {
                            store
                                .get::<Document>(
                                    &format!("task:{}:{}", homework.id, game.identifier),
                                    Duration::MAX,
                                )
                                .await
                                .map(|cached| cached.value.details)
                        };
                        if let Some(details) = details {
                            statuses.push((game.identifier.clone(), details.evaluation.status));
                        }
                    }
                    let c2 = c.clone();
                    let id = homework.id;
                    let _ = ui.upgrade_in_event_loop(move |app| {
                        if c2.prefetch_version.load(Ordering::SeqCst) != version {
                            return;
                        }
                        // A live editor/in-memory result is newer than disk.
                        if c2
                            .state
                            .lock()
                            .expect("UI state")
                            .homework_progress
                            .contains_key(&id)
                        {
                            return;
                        }
                        c2.set_homework_games(&app, id, &games, false);
                        for (game, status) in statuses {
                            c2.update_homework_progress(&app, id, &game, status);
                        }
                    });
                }
            }
            // Priority pass: finish the visible page's evaluation metadata
            // before any code downloads or pagination into other pages.
            for homework in &homeworks {
                while c.operation.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                let games = match workspace.games(homework, force).await {
                    Ok(games) => games,
                    Err(Error::SessionExpired) => return,
                    Err(_) => continue,
                };
                if games.is_empty() {
                    if let (Some(client), Some(shixun)) = (&client, &homework.shixun_identifier) {
                        let instance = client.get_shixun(shixun).await.ok();
                        if let Ok(challenges) = client.get_challenges(shixun).await {
                            let confirmed = homework.myshixun_identifier.is_none()
                                && instance.is_some_and(|instance| instance.myshixun_id.is_none());
                            let total = challenges.len();
                            let c2 = c.clone();
                            let id = homework.id;
                            let _ = ui.upgrade_in_event_loop(move |app| {
                                if c2.prefetch_version.load(Ordering::SeqCst) == version {
                                    c2.set_unstarted_progress(&app, id, total, confirmed);
                                }
                            });
                        }
                    }
                    continue;
                }
                let c2 = c.clone();
                let id = homework.id;
                let rows = games.clone();
                let _ = ui.upgrade_in_event_loop(move |app| {
                    if c2.prefetch_version.load(Ordering::SeqCst) == version {
                        // Keep known green rows visible while revalidating.
                        c2.set_homework_games(&app, id, &rows, false);
                    }
                });
                for game in games {
                    while c.operation.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    let details = match workspace.task_details(id, &game.identifier, force).await {
                        Ok(details) => details,
                        Err(Error::SessionExpired) => return,
                        Err(_) => continue,
                    };
                    let c2 = c.clone();
                    let _ = ui.upgrade_in_event_loop(move |app| {
                        if c2.prefetch_version.load(Ordering::SeqCst) == version {
                            c2.update_homework_progress(
                                &app,
                                id,
                                &game.identifier,
                                details.evaluation.status,
                            );
                        }
                    });
                }
            }
            let mut loaded = 0;
            let mut failed = 0;
            let mut next_page = 1;
            let mut more_pages = !(current_page == 1 && homeworks.len() < 20);
            let refresh_ids = homeworks
                .iter()
                .map(|homework| homework.id)
                .collect::<std::collections::HashSet<_>>();
            let mut queue = std::collections::VecDeque::from(homeworks);
            let mut seen = std::collections::HashSet::new();
            loop {
                let homework = if let Some(homework) = queue.pop_front() {
                    homework
                } else {
                    if !more_pages {
                        break;
                    }
                    if next_page == current_page {
                        next_page += 1;
                    }
                    let Some(course) = course else { break };
                    while c.operation.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    match workspace.homework_page(course, next_page).await {
                        Ok(page) => {
                            more_pages = page.items.len() == 20
                                && page
                                    .total
                                    .is_none_or(|total| u64::from(next_page) * 20 < total);
                            next_page += 1;
                            queue.extend(page.items.into_iter().filter(|h| !seen.contains(&h.id)));
                            if queue.is_empty() {
                                break;
                            }
                            continue;
                        }
                        Err(_) => {
                            failed += 1;
                            break;
                        }
                    }
                };
                if !seen.insert(homework.id) {
                    continue;
                }
                // A manual refresh revalidates the visible page. Other pages
                // continue their ordinary cached prefetch instead of redownloading
                // every code file in the course.
                let force = force && refresh_ids.contains(&homework.id);
                while c.operation.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                let games = match workspace.games(&homework, false).await {
                    Ok(games) => games,
                    Err(Error::SessionExpired) => break,
                    Err(_) => {
                        failed += 1;
                        continue;
                    }
                };
                if games.is_empty() {
                    if let (Some(client), Some(shixun)) = (&client, &homework.shixun_identifier) {
                        let instance = client.get_shixun(shixun).await.ok();
                        if let Ok(challenges) = client.get_challenges(shixun).await {
                            let total = challenges.len();
                            let confirmed = homework.myshixun_identifier.is_none()
                                && instance.is_some_and(|instance| instance.myshixun_id.is_none());
                            let c2 = c.clone();
                            let id = homework.id;
                            let _ = ui.upgrade_in_event_loop(move |app| {
                                if c2.prefetch_version.load(Ordering::SeqCst) == version {
                                    c2.set_unstarted_progress(&app, id, total, confirmed);
                                }
                            });
                        }
                    }
                } else {
                    let c2 = c.clone();
                    let id = homework.id;
                    let progress_games = games.clone();
                    let _ = ui.upgrade_in_event_loop(move |app| {
                        if c2.prefetch_version.load(Ordering::SeqCst) == version {
                            c2.set_homework_games(&app, id, &progress_games, false);
                        }
                    });
                }
                for game in games {
                    while c.operation.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    let status = match workspace
                        .document(homework.id, &game.identifier, force)
                        .await
                    {
                        Ok(document) => {
                            loaded += 1;
                            Some(document.details.evaluation.status)
                        }
                        Err(Error::SessionExpired) => return,
                        Err(_) => {
                            failed += 1;
                            None
                        }
                    };
                    let c2 = c.clone();
                    let id = homework.id;
                    let priority_loaded = refresh_ids.contains(&id);
                    let game_id = game.identifier.clone();
                    let _ = ui.upgrade_in_event_loop(move |app| {
                        if c2.prefetch_version.load(Ordering::SeqCst) == version {
                            if let Some(status) = status.filter(|_| !priority_loaded) {
                                c2.update_homework_progress(&app, id, &game_id, status);
                            }
                            app.set_cache_status(
                                format!(
                                    "后台已缓存 {loaded} 道题{}",
                                    if failed > 0 {
                                        format!(" · {failed} 项暂未获取")
                                    } else {
                                        String::new()
                                    }
                                )
                                .into(),
                            );
                        }
                    });
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            let _ = ui.upgrade_in_event_loop(move |app| {
                if c.prefetch_version.load(Ordering::SeqCst) == version {
                    app.set_cache_status(
                        format!(
                            "后台缓存完成：{loaded} 道题{}",
                            if failed > 0 {
                                format!(" · {failed} 项暂未获取")
                            } else {
                                String::new()
                            }
                        )
                        .into(),
                    );
                }
            });
        });
        *self.prefetch_job.lock().expect("prefetch job") = Some(job);
    }
    pub(super) fn has_editor(&self) -> bool {
        let state = self.workbench.lock().expect("workbench");
        state.document.is_some() && state.ready
    }
    pub(super) fn send_web(&self, app: &AppWindow, value: Value) {
        self.web_outbox
            .lock()
            .expect("web outbox")
            .push_back(value.clone());
        publish(app, value);
    }
    pub(crate) fn drain_web_messages(&self) -> Vec<Value> {
        self.web_outbox
            .lock()
            .expect("web outbox")
            .drain(..)
            .collect()
    }

    pub(super) fn clear_workbench(&self, app: &AppWindow) {
        let (version, ready) = {
            let state = self.workbench.lock().expect("workbench");
            (state.version + 1, state.ready)
        };
        *self.workbench.lock().expect("workbench") = WorkbenchState {
            version,
            ready,
            ..Default::default()
        };
        self.send_web(app, json!({"type":"clear"}));
    }

    pub fn request_close(self: &Arc<Self>) {
        let Some(app) = self.ui.upgrade() else { return };
        if self.operation.load(Ordering::SeqCst) {
            self.send_web(
                &app,
                json!({"type":"error","message":"当前操作尚未结束，请完成后关闭窗口。"}),
            );
            return;
        }
        if self.has_editor() {
            self.send_web(&app, json!({"type":"before-close"}));
        } else {
            let _ = slint::quit_event_loop();
        }
    }

    pub(super) fn open_homework(self: &Arc<Self>, id: u64) {
        let Some(app) = self.ui.upgrade() else { return };
        if self.has_editor() {
            self.send_web(&app, json!({"type":"navigate","target":{"homework":id}}));
            return;
        }
        self.open_homework_direct(id);
    }

    fn open_homework_direct(self: &Arc<Self>, id: u64) {
        let Some(app) = self.ui.upgrade() else { return };
        let (homework, client, cache) = {
            let state = self.state.lock().expect("UI state");
            (
                state.homeworks.iter().find(|h| h.id == id).cloned(),
                state.client.clone(),
                state.cache.clone(),
            )
        };
        let (Some(homework), Some(client)) = (homework, client) else {
            return;
        };
        let workspace = self.workspace_cache();
        if homework.shixun_identifier.is_none() {
            app.set_notice("该作业不包含可打开的实训代码工作区。".into());
            return;
        }
        if self.operation.load(Ordering::SeqCst) {
            app.set_notice("请等待当前保存、评测或本地运行完成后再切换作业。".into());
            return;
        }
        self.send_web(&app, json!({"type":"clear"}));
        app.set_page(2);
        let version = {
            let mut state = self.workbench.lock().expect("workbench");
            state.version += 1;
            state.homework = Some(homework.clone());
            state.document = None;
            state.games.clear();
            state.version
        };
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let key = format!("games:{}", homework.id);
            if let Some(store) = &cache
                && let Some(cached) = store.get::<Vec<Game>>(&key, Duration::from_secs(600)).await
            {
                let fresh = !cached.stale;
                let games = cached.value;
                let c2 = c.clone();
                let h = homework.clone();
                let _ =
                    ui.upgrade_in_event_loop(move |app| c2.show_games(&app, version, &h, games));
                if fresh {
                    return;
                }
            }
            let result = async {
                if let Some(workspace) = workspace {
                    return workspace.games(&homework, false).await;
                }
                if let Some(instance) = &homework.myshixun_identifier {
                    client.get_my_challenges(instance).await
                } else {
                    let shixun = client
                        .get_shixun(
                            homework
                                .shixun_identifier
                                .as_deref()
                                .ok_or(Error::InvalidInput("缺少实训标识符"))?,
                        )
                        .await?;
                    if let Some(instance) = shixun.myshixun_id {
                        client.get_my_challenges(&instance.to_string()).await
                    } else {
                        Ok(Vec::new())
                    }
                }
            }
            .await;
            if let (Ok(games), Some(store)) = (&result, &cache) {
                let _ = store.put(&key, games).await;
            }
            let _ = ui.upgrade_in_event_loop(move |app| {
                if c.workbench.lock().expect("workbench").version != version {
                    return;
                }
                match result {
                    Ok(games) => c.show_games(&app, version, &homework, games),
                    Err(error) => c.fail(&app, error),
                }
            });
        });
    }

    fn show_games(
        self: &Arc<Self>,
        app: &AppWindow,
        version: u64,
        homework: &Homework,
        games: Vec<Game>,
    ) {
        let game = {
            let mut state = self.workbench.lock().expect("workbench");
            if state.version != version {
                return;
            }
            state.games = games.clone();
            state
                .document
                .as_ref()
                .map(|d| d.details.task.game_identifier.clone())
                .filter(|id| games.iter().any(|g| g.identifier == *id))
                .or_else(|| games.first().map(|g| g.identifier.clone()))
        };
        if !games.is_empty() {
            self.set_homework_games(app, homework.id, &games, false);
        }
        self.send_web(app,json!({"type":"homework","title":homework.name,"games":games,"game":game,"can_enter":games.is_empty()}));
        if let Some(game) = game
            && self.workbench.lock().expect("workbench").document.is_none()
        {
            self.load_game(&game, false, None);
        }
    }

    fn load_game(self: &Arc<Self>, game: &str, force: bool, reply: Option<String>) {
        let (client, cache) = {
            let state = self.state.lock().expect("UI state");
            (state.client.clone(), state.cache.clone())
        };
        let Some(client) = client else { return };
        let workspace = self.workspace_cache();
        let (homework_id, version) = {
            let mut state = self.workbench.lock().expect("workbench");
            if !state.games.iter().any(|g| g.identifier == game) {
                return;
            }
            state.version += 1;
            (
                state.homework.as_ref().map(|h| h.id).unwrap_or_default(),
                state.version,
            )
        };
        let game = game.to_owned();
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let key = format!("task:{homework_id}:{game}");
            if !force
                && let Some(store) = &cache
                && let Some(cached) = store.get::<Document>(&key, Duration::from_secs(600)).await
            {
                let fresh = !cached.stale;
                let draft = store
                    .get::<Draft>(&format!("draft:{game}"), Duration::MAX)
                    .await
                    .map(|v| v.value)
                    .unwrap_or_default();
                let pending = store
                    .get::<bool>(&format!("pending:{game}"), Duration::MAX)
                    .await
                    .is_some();
                let c2 = c.clone();
                let document = cached.value;
                let _ = ui.upgrade_in_event_loop(move |app| {
                    c2.show_document(
                        &app,
                        version,
                        document,
                        draft,
                        DisplayOptions {
                            cached: true,
                            pending,
                            force: false,
                        },
                    )
                });
                if fresh {
                    return;
                }
            }
            let manages_cache = workspace.is_some();
            let result = async {
                if let Some(workspace) = workspace {
                    return workspace.document(homework_id, &game, force).await;
                }
                let details = client.get_task_details(&game, homework_id).await?;
                let mut files = Vec::new();
                let mut size = 0usize;
                for path in &details.task.editable_paths {
                    if files.iter().any(|f: &CodeFile| f.path == *path) {
                        continue;
                    }
                    let file = client.get_file_content(&game, path).await?;
                    size += file.content.len();
                    if size > 16 * 1024 * 1024 {
                        return Err(Error::ResponseTooLarge);
                    }
                    files.push(file);
                }
                Ok::<_, Error>(Document { details, files })
            }
            .await;
            let mut draft = Draft::default();
            let mut pending = false;
            if let Some(store) = &cache {
                if !manages_cache && let Ok(document) = &result {
                    let _ = store.put(&key, document).await;
                }
                draft = store
                    .get::<Draft>(&format!("draft:{game}"), Duration::MAX)
                    .await
                    .map(|v| v.value)
                    .unwrap_or_default();
                pending = store
                    .get::<bool>(&format!("pending:{game}"), Duration::MAX)
                    .await
                    .is_some();
                // A full fresh file snapshot reconciles an uncertain background save.
                // Never clear a submission marker on the strength of a code read alone.
                if result.is_ok()
                    && store
                        .get::<bool>(&format!("pending-sync:{game}"), Duration::MAX)
                        .await
                        .is_some()
                {
                    if store.remove(&format!("pending:{game}")).await.is_ok() {
                        let _ = store.remove(&format!("pending-sync:{game}")).await;
                        pending = false;
                    }
                }
            }
            let _ = ui.upgrade_in_event_loop(move |app| {
                if c.workbench.lock().expect("workbench").version != version {
                    return;
                }
                match result {
                    Ok(document) => {
                        c.show_document(
                            &app,
                            version,
                            document,
                            draft,
                            DisplayOptions {
                                cached: false,
                                pending,
                                force: false,
                            },
                        );
                        if let Some(id) = reply {
                            c.reply(&app, &id, Ok(json!({})));
                        }
                    }
                    Err(error) => {
                        if let Some(id) = reply {
                            c.reply(&app, &id, Err(error));
                        } else {
                            c.fail(&app, error);
                        }
                    }
                }
            });
        });
    }

    fn show_document(
        &self,
        app: &AppWindow,
        version: u64,
        document: Document,
        draft: Draft,
        options: DisplayOptions,
    ) {
        let DisplayOptions {
            cached,
            pending,
            force,
        } = options;
        let conflict = draft_conflicts(&document, &draft);
        let message = document_message(&document, &draft, cached, pending, conflict, force);
        let progress = {
            let mut state = self.workbench.lock().expect("workbench");
            if state.version != version {
                return;
            }
            let progress = state.homework.as_ref().map(|homework| {
                (
                    homework.id,
                    document.details.task.game_identifier.clone(),
                    document.details.evaluation.status,
                )
            });
            state.document = Some(document);
            state.pending_write = pending;
            progress
        };
        if let Some((id, game, status)) = progress {
            self.update_homework_progress(app, id, &game, status);
        }
        self.send_web(app, message);
    }

    pub(super) fn handle_web_event(self: &Arc<Self>, message: &str) {
        if message.len() > 20 * 1024 * 1024 {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message>(message) else {
            return;
        };
        let Some(app) = self.ui.upgrade() else { return };
        match message.action.as_str() {
            "celebrate-ac" => {
                let valid = self
                    .workbench
                    .lock()
                    .expect("workbench")
                    .document
                    .as_ref()
                    .is_some_and(|d| {
                        d.details.task.game_identifier == message.context
                            && d.details.evaluation.status == 2
                    });
                if valid {
                    app.set_celebration_game(message.context.into());
                    app.set_celebration_active(true);
                    app.set_celebration_progress(0.0);
                    app.set_celebration_id(app.get_celebration_id().wrapping_add(1));
                }
            }
            "stop-celebration" => app.set_celebration_active(false),
            "answers" => {
                let Some(id) = message.id else { return };
                let course_id = self.state.lock().expect("UI state").course_id;
                let query = {
                    let state = self.workbench.lock().expect("workbench");
                    state
                        .document
                        .as_ref()
                        .zip(state.homework.as_ref())
                        .and_then(|(d, h)| {
                            (d.details.task.game_identifier == message.context).then_some(
                                educoder_core::answers::AnswerQuery {
                                    course_id: course_id?,
                                    homework_id: h.id,
                                    position: d.details.task.position,
                                },
                            )
                        })
                };
                let Some(query) = query else {
                    self.reply(&app, &id, Err(Error::InvalidInput("当前题目已切换")));
                    return;
                };
                let refresh = message.payload["refresh"].as_bool().unwrap_or(false);
                let c = self.clone();
                let ui = self.ui.clone();
                self.runtime.spawn(async move {
                    let result = async {
                        let client = educoder_core::answers::AnswerClient::new()?;
                        let answer = client.lookup(&query, refresh).await?;
                        serde_json::to_value(answer).map_err(|_| Error::Response("答案数据"))
                    }
                    .await;
                    let _ = ui.upgrade_in_event_loop(move |app| c.reply(&app, &id, result));
                });
            }
            "ready" => {
                self.workbench.lock().expect("workbench").ready = true;
                let state = self.workbench.lock().expect("workbench");
                let h = state.homework.clone();
                let games = state.games.clone();
                let document = state.document.clone();
                let pending = state.pending_write;
                drop(state);
                if let Some(homework) = h {
                    self.send_web(&app,json!({"type":"homework","title":homework.name,"games":games,"game":document.as_ref().map(|d|&d.details.task.game_identifier),"can_enter":games.is_empty()}));
                }
                if let Some(document) = document {
                    self.send_document_from_cache(document, pending);
                }
            }
            "close-ready" => {
                let _ = slint::quit_event_loop();
            }
            "logout-ready" => self.logout_direct(),
            "navigate" => {
                if self.operation.load(Ordering::SeqCst) {
                    self.send_web(
                        &app,
                        json!({"type":"error","message":"请等待当前操作完成后再切换。"}),
                    );
                    return;
                }
                app.set_page(0);
                if let Some(course) = message.payload.get("course").and_then(Value::as_u64) {
                    self.select_course_direct(course);
                }
                if let Some(homework) = message.payload.get("homework").and_then(Value::as_u64) {
                    self.open_homework_direct(homework);
                }
            }
            "settings" => {
                app.set_settings_return_page(app.get_page());
                app.set_page(1);
            }
            "select-game" => {
                if !self.operation.load(Ordering::SeqCst)
                    && let Some(game) = message.payload.get("game").and_then(Value::as_str)
                {
                    self.load_game(game, false, None);
                }
            }
            "draft" => self.save_draft(message),
            "refresh" => self.load_game(&message.context, true, message.id),
            "enter" => self.enter_workbench(message),
            "resolve-conflict" | "discard-draft" => self.resolve_draft(message),
            "sync" | "submit" | "run" | "result" => self.perform_operation(message),
            _ => {}
        }
    }

    fn send_document_from_cache(self: &Arc<Self>, document: Document, pending: bool) {
        let cache = self.state.lock().expect("UI state").cache.clone();
        let version = self.workbench.lock().expect("workbench").version;
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let draft = if let Some(store) = cache {
                store
                    .get::<Draft>(
                        &format!("draft:{}", document.details.task.game_identifier),
                        Duration::MAX,
                    )
                    .await
                    .map(|v| v.value)
                    .unwrap_or_default()
            } else {
                Draft::default()
            };
            let _ = ui.upgrade_in_event_loop(move |app| {
                c.show_document(
                    &app,
                    version,
                    document,
                    draft,
                    DisplayOptions {
                        cached: true,
                        pending,
                        force: false,
                    },
                )
            });
        });
    }

    fn save_draft(self: &Arc<Self>, message: Message) {
        let Some(id) = message.id else { return };
        let Some(app) = self.ui.upgrade() else { return };
        let (cache, document) = {
            let cache = self.state.lock().expect("UI state").cache.clone();
            let document = self.workbench.lock().expect("workbench").document.clone();
            (cache, document)
        };
        let Some(document) = document else {
            self.reply(&app, &id, Err(Error::InvalidInput("题目尚未加载")));
            return;
        };
        if message.context != document.details.task.game_identifier {
            self.reply(&app, &id, Err(Error::InvalidInput("题目已切换")));
            return;
        }
        let result = serde_json::from_value::<Draft>(message.payload)
            .map_err(|_| Error::InvalidInput("草稿格式无效"));
        let draft = match result {
            Ok(draft) => draft,
            Err(error) => {
                self.reply(&app, &id, Err(error));
                return;
            }
        };
        if let Err(error) = validate_text_files(&document, &draft.files) {
            self.reply(&app, &id, Err(error));
            return;
        }
        let Some(cache) = cache else {
            self.reply(&app, &id, Err(Error::InvalidInput("本地缓存目录不可用")));
            return;
        };
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let result = cache
                .put(&format!("draft:{}", message.context), &draft)
                .await
                .map(|_| json!({}));
            let _ = ui.upgrade_in_event_loop(move |app| c.reply(&app, &id, result));
        });
    }

    fn resolve_draft(self: &Arc<Self>, message: Message) {
        let Some(id) = message.id else { return };
        let Some(_app) = self.ui.upgrade() else {
            return;
        };
        let document = self.workbench.lock().expect("workbench").document.clone();
        let cache = self.state.lock().expect("UI state").cache.clone();
        let (Some(document), Some(cache)) = (document, cache) else {
            return;
        };
        if document.details.task.game_identifier != message.context {
            return;
        }
        let discard = message.action == "discard-draft";
        let c = self.clone();
        let ui = self.ui.clone();
        let version = self.workbench.lock().expect("workbench").version;
        self.runtime.spawn(async move {
            let key = format!("draft:{}", message.context);
            let mut draft = cache
                .get::<Draft>(&key, Duration::MAX)
                .await
                .map(|v| v.value)
                .unwrap_or_default();
            if discard {
                draft.files = text_files(&document.files);
            }
            draft.baseline = text_files(&document.files);
            draft.conflict = false;
            let result = cache.put(&key, &draft).await;
            let _ = ui.upgrade_in_event_loop(move |app| {
                if discard && result.is_ok() {
                    c.show_document(
                        &app,
                        version,
                        document,
                        draft,
                        DisplayOptions {
                            cached: false,
                            pending: false,
                            force: true,
                        },
                    );
                }
                c.reply(&app, &id, result.map(|_| json!({})));
            });
        });
    }

    fn enter_workbench(self: &Arc<Self>, message: Message) {
        let Some(id) = message.id else { return };
        let Some(app) = self.ui.upgrade() else { return };
        if !app.get_connected() {
            self.reply(&app, &id, Err(Error::SessionExpired));
            return;
        }
        let homework = self.workbench.lock().expect("workbench").homework.clone();
        let client = self.state.lock().expect("UI state").client.clone();
        let (Some(homework), Some(client)) = (homework, client) else {
            return;
        };
        let Some(shixun) = homework.shixun_identifier.clone() else {
            return;
        };
        if self
            .operation
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            self.reply(&app, &id, Err(Error::InvalidInput("请等待当前操作完成")));
            return;
        }
        let guard = OperationGuard(self.operation.clone());
        app.set_workspace_busy(true);
        let cache = self.state.lock().expect("UI state").cache.clone();
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            // Invalidate even after an uncertain response: entering may have succeeded.
            if let Some(cache) = cache {
                let _ = cache.remove(&format!("games:{}", homework.id)).await;
            }
            let result = client.enter_shixun_for_homework(&shixun, homework.id).await;
            drop(guard);
            let _ = ui.upgrade_in_event_loop(move |app| {
                app.set_workspace_busy(false);
                match result {
                    Ok(_) => {
                        c.reply(&app, &id, Ok(json!({})));
                        c.open_homework_direct(homework.id);
                    }
                    Err(error) => c.reply(&app, &id, Err(error)),
                }
            });
        });
    }

    fn perform_operation(self: &Arc<Self>, message: Message) {
        let Some(id) = message.id else { return };
        let Some(app) = self.ui.upgrade() else { return };
        let (document, homework_id) = {
            let state = self.workbench.lock().expect("workbench");
            (
                state.document.clone(),
                state.homework.as_ref().map(|h| h.id),
            )
        };
        let (client, cache) = {
            let state = self.state.lock().expect("UI state");
            (state.client.clone(), state.cache.clone())
        };
        let (Some(document), Some(client)) = (document, client) else {
            return;
        };
        if message.context != document.details.task.game_identifier {
            self.reply(&app, &id, Err(Error::InvalidInput("题目已切换")));
            return;
        }
        if message.action != "run" && !app.get_connected() {
            self.reply(&app, &id, Err(Error::SessionExpired));
            return;
        }
        if self
            .operation
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            self.reply(&app, &id, Err(Error::InvalidInput("请等待当前操作完成")));
            return;
        }
        let guard = OperationGuard(self.operation.clone());
        app.set_workspace_busy(true);
        let tools = self
            .preferences
            .lock()
            .expect("preferences")
            .toolchains
            .clone();
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            let _guard = guard;
            let outcome = async {
                if message.action == "result" {
                    let mut previous = document.details.clone();
                    if let Some(store) = &cache
                        && store.get::<bool>(&format!("pending:{}", message.context), Duration::MAX).await.is_some() {
                        previous.evaluation.status = 1;
                    }
                    let details = client
                        .get_evaluation(&previous)
                        .await.map_err(|error| if previous.evaluation.status==1 && !matches!(error,Error::SessionExpired) {Error::EvaluationRead(Box::new(error))}else{error})?;
                    if let (Some(store), Some(homework_id)) = (&cache, homework_id) {
                        let _ = store.put(
                            &format!("task-details:{homework_id}:{}", message.context),
                            &details,
                        ).await;
                    }
                    if details.evaluation.status != 1
                        && let Some(store) = &cache {
                            store
                                .remove(&format!("pending:{}", message.context))
                                .await?;
                        }
                    return Ok::<_, Error>((
                        json!({"evaluation":details.evaluation,"can_submit":details.submission.is_some()&&details.evaluation.status!=1}),
                        Some(details),
                        None,
                    ));
                }
                let files = serde_json::from_value::<Vec<TextFile>>(
                    message.payload.get("files").cloned().unwrap_or(Value::Null),
                )
                .map_err(|_| Error::InvalidInput("代码文件格式无效"))?;
                validate_text_files(&document, &files)?;
                let merged = merge_files(&document.files, &files);
                if message.action == "run" {
                    let samples = serde_json::from_value::<Vec<Sample>>(
                        message
                            .payload
                            .get("samples")
                            .cloned()
                            .unwrap_or(Value::Null),
                    )
                    .map_err(|_| Error::InvalidInput("本地样例格式无效"))?;
                    let entry = message
                        .payload
                        .get("entry")
                        .and_then(Value::as_str)
                        .ok_or(Error::InvalidInput("请选择入口文件"))?;
                    let result =
                        run_samples(&merged, entry, &samples, &tools, Duration::from_secs(5))
                            .await?;
                    return Ok((
                        serde_json::to_value(result)
                            .map_err(|_| Error::Response("运行结果序列化失败"))?,
                        None,
                        None,
                    ));
                }
                if let Some(store) = &cache {
                    if store
                        .get::<bool>(&format!("pending:{}", message.context), Duration::MAX)
                        .await
                        .is_some()
                    {
                        return Err(Error::WriteUncertain);
                    }
                    if let Some(draft) = store
                        .get::<Draft>(&format!("draft:{}", message.context), Duration::MAX)
                        .await
                        && draft_conflicts(&document, &draft.value) {
                            return Err(Error::Conflict);
                        }
                }
                let details = prepare_write(&client, &document, &files).await?;
                if let Some(workspace)=c.workspace_cache(){workspace.invalidate_reads().await;}
                if let Some(store) = &cache {
                    if message.action == "sync" {
                        store.put(&format!("pending-sync:{}", message.context), &true).await?;
                    } else {
                        store.remove(&format!("pending-sync:{}", message.context)).await?;
                    }
                    store
                        .put(&format!("pending:{}", message.context), &true)
                        .await?;
                }
                let write = if message.action == "submit" {
                    let editable=text_code_files(&files);
                    client.submit_code(&details, &editable).await
                } else {
                    let editable=text_code_files(&files);
                    client
                        .save_files(&details, &editable, false)
                        .await
                        .map(|_| ())
                };
                if let Err(error) = write {
                    if !matches!(error, Error::WriteUncertain)
                        && let Some(store) = &cache {
                            let _ = store.remove(&format!("pending:{}", message.context)).await;
                            let _ = store.remove(&format!("pending-sync:{}", message.context)).await;
                        }
                    return Err(error);
                }
                let mut result_error = None;
                let result = if message.action == "submit" {
                    match client
                        .wait_for_evaluation(&details, Duration::from_secs(60))
                        .await {
                        Ok(result) => result,
                        Err(Error::EvaluationPending) => {
                            let mut pending = details;
                            pending.evaluation.status = 1;
                            pending
                        }
                        Err(Error::SessionExpired) => return Err(Error::SessionExpired),
                        Err(error) => {
                            result_error = Some(format!("代码已提交，读取评测结果失败：{error}。正在继续查询，请勿重复提交。"));
                            let mut pending = details;
                            pending.evaluation.status = 1;
                            pending
                        }
                    }
                } else {
                    details
                };
                if let Some(store) = &cache {
                    if let Some(homework_id) = homework_id {
                        let _ = store.put(
                            &format!("task-details:{homework_id}:{}", message.context),
                            &result,
                        ).await;
                    }
                    if result.evaluation.status != 1 { store
                        .remove(&format!("pending:{}", message.context))
                        .await?; }
                    if message.action == "sync" {
                        let _ = store.remove(&format!("pending-sync:{}", message.context)).await;
                    }
                    let _ = store
                        .put(
                            &format!(
                                "task:{}:{}",
                                result
                                    .submission
                                    .as_ref()
                                    .map(|c| c.homework_id)
                                    .unwrap_or_default(),
                                message.context
                            ),
                            &Document {
                                details: result.clone(),
                                files: merged.clone(),
                            },
                        )
                        .await;
                }
                Ok((
                    json!({"evaluation":result.evaluation,"can_submit":result.submission.is_some()&&result.evaluation.status!=1,"result_error":result_error}),
                    Some(result),
                    Some(merged),
                ))
            }
            .await;
            let _ = ui.upgrade_in_event_loop(move |app| {
                app.set_workspace_busy(false);
                match outcome {
                Ok((value, details, files)) => {
                    {
                        let mut state = c.workbench.lock().expect("workbench");
                        if let Some(doc) = &mut state.document
                            && doc.details.task.game_identifier == message.context {
                                if let Some(details) = details {
                                    doc.details = details;
                                }
                                if let Some(files) = files {
                                    doc.files = files;
                                }
                            }
                        state.pending_write = state.document.as_ref().is_some_and(|d| d.details.evaluation.status == 1);
                    }
                    let progress = {
                        let state = c.workbench.lock().expect("workbench");
                        state.homework.as_ref().zip(state.document.as_ref()).map(|(homework, doc)|
                            (homework.id, doc.details.task.game_identifier.clone(), doc.details.evaluation.status))
                    };
                    if let Some((homework, game, status)) = progress {
                        c.update_homework_progress(&app, homework, &game, status);
                    }
                    c.reply(&app, &id, Ok(value));
                }
                Err(error) => {
                    if matches!(error, Error::SessionExpired) {
                        c.fail(&app, Error::SessionExpired);
                    }
                    c.reply(&app, &id, Err(error));
                }
            }
            });
        });
    }

    fn reply(&self, app: &AppWindow, id: &str, result: Result<Value>) {
        match result {
            Ok(data) => self.send_web(app, json!({"reply_to":id,"ok":true,"data":data})),
            Err(error) => self.send_web(
                app,
                json!({"reply_to":id,"ok":false,"error":error.to_string()}),
            ),
        }
    }
}

struct OperationGuard(Arc<std::sync::atomic::AtomicBool>);
impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn text_files(files: &[CodeFile]) -> Vec<TextFile> {
    files
        .iter()
        .filter_map(|f| {
            f.text().ok().map(|text| TextFile {
                path: f.path.clone(),
                content: text.into(),
            })
        })
        .collect()
}

fn text_code_files(files: &[TextFile]) -> Vec<CodeFile> {
    files
        .iter()
        .map(|f| CodeFile {
            path: f.path.clone(),
            content: f.content.as_bytes().to_vec(),
        })
        .collect()
}
fn merge_files(base: &[CodeFile], edits: &[TextFile]) -> Vec<CodeFile> {
    base.iter()
        .map(|file| CodeFile {
            path: file.path.clone(),
            content: edits
                .iter()
                .find(|f| f.path == file.path)
                .map(|f| f.content.as_bytes().to_vec())
                .unwrap_or_else(|| file.content.clone()),
        })
        .collect()
}
fn validate_text_files(document: &Document, files: &[TextFile]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    let mut total = 0usize;
    for file in files {
        if !document.details.task.editable_paths.contains(&file.path) || !seen.insert(&file.path) {
            return Err(Error::InvalidInput("文件不属于当前题目或存在重复路径"));
        }
        total = total
            .checked_add(file.content.len())
            .ok_or(Error::ResponseTooLarge)?;
        if total > 16 * 1024 * 1024 {
            return Err(Error::ResponseTooLarge);
        }
    }
    Ok(())
}
fn draft_conflicts(document: &Document, draft: &Draft) -> bool {
    draft.conflict
        || draft.files.iter().any(|edited| {
            let base = draft.baseline.iter().find(|f| f.path == edited.path);
            let remote = document
                .files
                .iter()
                .find(|f| f.path == edited.path)
                .and_then(|f| f.text().ok());
            match (base, remote) {
                (Some(base), Some(remote)) => {
                    edited.content != base.content
                        && remote != base.content
                        && edited.content != remote
                }
                _ => true,
            }
        })
}
fn document_message(
    document: &Document,
    draft: &Draft,
    cached: bool,
    pending: bool,
    conflict: bool,
    force: bool,
) -> Value {
    let files = document
        .files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "content": file.text().unwrap_or("此二进制文件无法在文本编辑器中编辑。"),
                "read_only": file.text().is_err()
            })
        })
        .collect::<Vec<_>>();
    json!({
        "type": "task",
        "task": {
            "game_identifier": document.details.task.game_identifier,
            "title": document.details.task.title,
            "description_markdown": document.details.task.description_markdown,
            "can_submit": document.details.submission.is_some() && !pending && document.details.evaluation.status != 1
        },
        "files": files, "drafts": draft.files, "samples": draft.samples,
        "evaluation": document.details.evaluation,
        "cached": cached, "conflict": conflict, "force": force
    })
}
async fn prepare_write(
    client: &EducoderClient,
    document: &Document,
    files: &[TextFile],
) -> Result<TaskDetails> {
    for file in files {
        let remote = client
            .get_file_content(&document.details.task.game_identifier, &file.path)
            .await?;
        let base = document
            .files
            .iter()
            .find(|f| f.path == file.path)
            .ok_or(Error::Conflict)?;
        if remote.content != base.content {
            return Err(Error::Conflict);
        }
    }
    let details = client
        .get_task_details(
            &document.details.task.game_identifier,
            document
                .details
                .submission
                .as_ref()
                .map(|c| c.homework_id)
                .unwrap_or_default(),
        )
        .await?;
    if details.evaluation.status == 1 {
        return Err(Error::EvaluationPending);
    }
    Ok(details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use educoder_core::models::{Evaluation, Task};

    fn document() -> Document {
        Document {
            details: TaskDetails {
                task: Task {
                    game_identifier: "demo".into(),
                    title: "Demo".into(),
                    position: 1,
                    description_markdown: String::new(),
                    editable_paths: vec!["main.rs".into(), "data.bin".into()],
                    shixun_name: None,
                    myshixun_identifier: None,
                },
                evaluation: Evaluation {
                    status: 0,
                    revision: None,
                    compile_output: String::new(),
                    tests: vec![],
                },
                submission: None,
            },
            files: vec![
                CodeFile {
                    path: "main.rs".into(),
                    content: b"remote".to_vec(),
                },
                CodeFile {
                    path: "data.bin".into(),
                    content: vec![255],
                },
            ],
        }
    }
    #[test]
    fn drafts_use_three_way_conflicts_and_preserve_binary_helpers() {
        let doc = document();
        let mut draft = Draft {
            files: vec![TextFile {
                path: "main.rs".into(),
                content: "local".into(),
            }],
            baseline: vec![TextFile {
                path: "main.rs".into(),
                content: "old".into(),
            }],
            ..Default::default()
        };
        assert!(draft_conflicts(&doc, &draft));
        draft.baseline[0].content = "remote".into();
        assert!(!draft_conflicts(&doc, &draft));
        assert!(validate_text_files(&doc, &draft.files).is_ok());
        let merged = merge_files(&doc.files, &draft.files);
        assert_eq!(merged[0].content, b"local");
        assert_eq!(merged[1].content, vec![255]);
        let frame = document_message(&doc, &draft, true, false, false, false);
        assert_eq!(frame["files"][1]["read_only"], true);
        assert!(frame.get("submission").is_none());
        draft.files.push(draft.files[0].clone());
        assert!(validate_text_files(&doc, &draft.files).is_err());
        draft.files[1].path = "../outside".into();
        assert!(validate_text_files(&doc, &draft.files).is_err());
    }
}
