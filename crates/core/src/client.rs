use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{
    Client, Method, Url,
    header::{self, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::{
    sync::{Mutex, OnceCell, RwLock},
    time::Instant,
};

use crate::{Error, Result, Session, SessionVault, dto::*, models::*, signing};

const ORIGIN: &str = "https://www.educoder.net/";
const API_ORIGIN: &str = "https://data.educoder.net/";

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub timeout: Duration,
    pub minimum_interval: Duration,
    pub max_response_bytes: usize,
}
impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            minimum_interval: Duration::from_millis(300),
            max_response_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Cloneable client; clones share rate limits, clock calibration and login state.
/// Requests never follow redirects or retry state-changing operations.
#[derive(Clone)]
pub struct EducoderClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: Client,
    origin: Url,
    session: RwLock<Option<Session>>,
    clock_offset: OnceCell<i64>,
    next_request: Mutex<Instant>,
    options: ClientOptions,
}

impl EducoderClient {
    pub fn new(session: Session) -> Result<Self> {
        Self::with_options(session, ClientOptions::default())
    }

    pub fn with_options(session: Session, options: ClientOptions) -> Result<Self> {
        if options.timeout.is_zero() || options.max_response_bytes == 0 {
            return Err(Error::InvalidInput(
                "timeout 和 max_response_bytes 必须大于零",
            ));
        }
        let http = Client::builder()
            .timeout(options.timeout)
            .connect_timeout(options.timeout.min(Duration::from_secs(10)))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .user_agent("educoder-plus-plus/0.1")
            .build()
            .map_err(Error::Transport)?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                origin: Url::parse(API_ORIGIN).expect("constant URL"),
                session: RwLock::new(Some(session)),
                clock_offset: OnceCell::new(),
                next_request: Mutex::new(Instant::now()),
                options,
            }),
        })
    }

    /// Check the account before exposing it to the UI or persisting it.
    pub async fn validate_session(&self) -> Result<User> {
        self.get_user_info().await
    }

    pub async fn replace_session(&self, session: Session) {
        *self.inner.session.write().await = Some(session);
    }

    /// Stop future requests on all clones, then remove the saved credential.
    /// Already running requests cannot be recalled.
    pub async fn logout(&self, vault: Option<&SessionVault>) -> Result<()> {
        *self.inner.session.write().await = None;
        if let Some(vault) = vault {
            vault.clear().await?;
        }
        Ok(())
    }

    pub async fn get_user_info(&self) -> Result<User> {
        let user: User = self
            .get::<UserDto>("/api/users/get_user_info.json", &[])
            .await?
            .into();
        if user.login.is_empty() {
            return Err(Error::SessionExpired);
        }
        Ok(user)
    }

    /// Matches the reference endpoint; pagination of this endpoint is unverified.
    pub async fn get_courses(&self) -> Result<Page<Course>> {
        let user = self.get_user_info().await?;
        self.get_courses_for(&user.login).await
    }

    pub async fn get_courses_for(&self, login: &str) -> Result<Page<Course>> {
        valid_id(login)?;
        self.get::<CoursesDto>(&format!("/api/users/{login}/courses.json"), &[])
            .await
            .map(Into::into)
    }

    pub async fn get_homeworks(
        &self,
        course_id: u64,
        kind: HomeworkKind,
        page: PageRequest,
    ) -> Result<Page<Homework>> {
        if course_id == 0 || page.page == 0 || !(1..=100).contains(&page.limit) {
            return Err(Error::InvalidInput("课程 ID、页码或分页大小无效"));
        }
        let kind = match kind {
            HomeworkKind::Text => "1",
            HomeworkKind::Shixun => "4",
        };
        self.get::<HomeworksDto>(
            &format!("/api/courses/{course_id}/homework_commons.json"),
            &[
                ("type", kind.into()),
                ("page", page.page.to_string()),
                ("limit", page.limit.to_string()),
            ],
        )
        .await
        .map(Into::into)
    }

    /// Read existing instances only; does not enter or create a shixun.
    pub async fn get_shixun(&self, shixun_identifier: &str) -> Result<Shixun> {
        valid_id(shixun_identifier)?;
        self.get::<ShixunDto>(&format!("/api/shixuns/{shixun_identifier}.json"), &[])
            .await
            .map(Into::into)
    }

    pub async fn get_challenges(&self, shixun_identifier: &str) -> Result<Vec<Challenge>> {
        valid_id(shixun_identifier)?;
        let response = self
            .get::<ChallengesDto>(
                &format!("/api/shixuns/{shixun_identifier}/challenges.json"),
                &[],
            )
            .await?;
        Ok(response
            .challenge_list
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Accepts either a personal instance identifier or its numeric ID as text.
    pub async fn get_my_challenges(&self, myshixun_identifier_or_id: &str) -> Result<Vec<Game>> {
        valid_id(myshixun_identifier_or_id)?;
        let response = self
            .get::<Vec<GameDto>>(
                &format!("/api/myshixuns/{myshixun_identifier_or_id}/challenges.json"),
                &[],
            )
            .await?;
        Ok(response.into_iter().map(Into::into).collect())
    }

    pub async fn get_task(&self, game_identifier: &str) -> Result<Task> {
        valid_id(game_identifier)?;
        self.get::<TaskDto>(&format!("/api/tasks/{game_identifier}.json"), &[])
            .await
            .map(|t| t.into_task(game_identifier))
    }

    pub async fn get_task_details(
        &self,
        game_identifier: &str,
        homework_id: u64,
    ) -> Result<TaskDetails> {
        valid_id(game_identifier)?;
        self.get::<TaskDetailsDto>(
            &format!("/api/tasks/{game_identifier}.json"),
            &[("homework_common_id", homework_id.to_string())],
        )
        .await
        .map(|dto| dto.into_details(game_identifier, homework_id))
    }

    /// Explicit save, restricted to files advertised as editable by this task.
    pub async fn save_files(
        &self,
        task: &TaskDetails,
        files: &[CodeFile],
        for_evaluation: bool,
    ) -> Result<SaveReceipt> {
        let context = task
            .submission
            .as_ref()
            .ok_or(Error::InvalidInput("该题缺少可提交的运行环境或上下文"))?;
        valid_id(&context.myshixun_identifier)?;
        if files.is_empty() {
            return Err(Error::InvalidInput("没有需要保存的代码文件"));
        }
        let mut seen = std::collections::HashSet::new();
        let mut total = 0usize;
        for file in files {
            valid_repo_path(&file.path)?;
            if !seen.insert(&file.path) {
                return Err(Error::InvalidInput("代码文件路径重复"));
            }
            total = total
                .checked_add(file.content.len())
                .ok_or(Error::ResponseTooLarge)?;
            if total > self.inner.options.max_response_bytes {
                return Err(Error::InvalidInput("代码总大小超过限制"));
            }
            if !task.task.editable_paths.contains(&file.path) {
                return Err(Error::InvalidInput("文件不属于当前题目的可编辑范围"));
            }
            if file.content.len() > self.inner.options.max_response_bytes || file.text().is_err() {
                return Err(Error::InvalidInput("代码文件过大或不是 UTF-8 文本"));
            }
        }
        let extras = serde_json::json!({
            "challenge_id": context.challenge_id,
            "homework_common_id": context.homework_id,
            "currentUserId": context.user_id,
            "exercise_id": "", "question_id": "", "subject_id": "", "competition_entry_id": ""
        });
        let mut receipt = None;
        for (index, file) in files.iter().enumerate() {
            let body = serde_json::json!({
                "path": file.path,
                "evaluate": u8::from(for_evaluation && index + 1 == files.len()),
                "content": file.text().map_err(|_| Error::InvalidInput("代码编码"))?,
                "game_id": context.game_id, "tab_type": context.tab_type,
                "homework_common_id": context.homework_id, "extras": extras
            });
            let response = self
                .request::<SaveDto>(
                    Method::POST,
                    &format!(
                        "/api/myshixuns/{}/update_file.json",
                        context.myshixun_identifier
                    ),
                    &[],
                    Some(&body),
                )
                .await
                .map_err(write_error)?
                .into_receipt();
            if response.commit_id.is_empty() {
                return Err(Error::WriteUncertain);
            }
            receipt = Some(response);
        }
        receipt.ok_or(Error::WriteUncertain)
    }

    /// Save all files, then start exactly one evaluation. Never retries writes.
    pub async fn submit_code(&self, task: &TaskDetails, files: &[CodeFile]) -> Result<()> {
        let receipt = self.save_files(task, files, true).await?;
        let context = task
            .submission
            .as_ref()
            .ok_or(Error::InvalidInput("缺少提交上下文"))?;
        if receipt.sec_key.is_empty() {
            return Err(Error::WriteUncertain);
        }
        let resubmit = if receipt.resubmit.is_null() {
            serde_json::json!("")
        } else {
            receipt.resubmit
        };
        let content_modified = if receipt.content_modified.is_null() {
            serde_json::json!(0)
        } else {
            receipt.content_modified
        };
        let body = serde_json::json!({
            "sec_key": receipt.sec_key, "resubmit": resubmit,
            "first": 1, "content_modified": content_modified,
            "shixun_environment_id": context.environment_id, "tab_type": context.tab_type,
            "extras": {
                "challenge_id": context.challenge_id, "commitID": receipt.commit_id,
                "homework_common_id": context.homework_id, "currentUserId": context.user_id,
                "exercise_id": "", "question_id": "", "subject_id": "", "competition_entry_id": ""
            }
        });
        self.request::<Value>(
            Method::POST,
            &format!("/api/tasks/{}/game_build.json", task.task.game_identifier),
            &[],
            Some(&body),
        )
        .await
        .map_err(write_error)?;
        Ok(())
    }

    pub async fn wait_for_evaluation(
        &self,
        previous: &TaskDetails,
        timeout: Duration,
    ) -> Result<TaskDetails> {
        let homework_id = previous
            .submission
            .as_ref()
            .map(|c| c.homework_id)
            .unwrap_or_default();
        let started = Instant::now();
        let mut saw_running = false;
        while started.elapsed() < timeout {
            tokio::time::sleep(
                Duration::from_secs(2).min(timeout.saturating_sub(started.elapsed())),
            )
            .await;
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            let current = tokio::time::timeout(
                remaining,
                self.get_task_details(&previous.task.game_identifier, homework_id),
            )
            .await
            .map_err(|_| Error::EvaluationPending)??;
            saw_running |= current.evaluation.status == 1;
            let changed = current
                .evaluation
                .revision
                .zip(previous.evaluation.revision)
                .is_some_and(|(now, old)| now > old)
                || current.evaluation != previous.evaluation;
            if matches!(current.evaluation.status, 2 | 3) && (saw_running || changed) {
                return Ok(current);
            }
        }
        Err(Error::EvaluationPending)
    }

    pub async fn get_file_content(&self, game_identifier: &str, path: &str) -> Result<CodeFile> {
        valid_id(game_identifier)?;
        valid_repo_path(path)?;
        let dto = self
            .get::<FileDto>(
                &format!("/api/tasks/{game_identifier}/rep_content.json"),
                &[("path", path.into())],
            )
            .await?;
        let encoded: String = dto
            .encoded()
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let content = STANDARD
            .decode(encoded)
            .map_err(|_| Error::Response("文件内容不是有效 Base64"))?;
        Ok(CodeFile {
            path: path.into(),
            content,
        })
    }

    /// Fetch description and all editable files, fail without replacing code with error text.
    /// No disk writes, no implicit instance creation.
    pub async fn get_task_snapshot(&self, game_identifier: &str) -> Result<TaskSnapshot> {
        let task = self.get_task(game_identifier).await?;
        let mut files = Vec::new();
        let mut size = 0usize;
        for path in &task.editable_paths {
            if files.iter().any(|f: &CodeFile| f.path == *path) {
                continue;
            }
            let file = self.get_file_content(game_identifier, path).await?;
            size = size
                .checked_add(file.content.len())
                .ok_or(Error::ResponseTooLarge)?;
            if size > self.inner.options.max_response_bytes {
                return Err(Error::ResponseTooLarge);
            }
            files.push(file);
        }
        Ok(TaskSnapshot { task, files })
    }

    pub async fn get_work_report(&self, student_work_id: u64) -> Result<WorkReport> {
        if student_work_id == 0 {
            return Err(Error::InvalidInput("作业报告 ID 必须大于零"));
        }
        self.get::<ReportDto>(
            &format!("/api/student_works/{student_work_id}/shixun_work_report.json"),
            &[],
        )
        .await
        .map(Into::into)
    }

    /// MUTATING: may create a new instance/repository even though the platform uses GET.
    /// Call only for an explicit user action; never automatically during crawling.
    pub async fn enter_shixun(&self, shixun_identifier: &str) -> Result<String> {
        valid_id(shixun_identifier)?;
        let response = self
            .get::<EnterDto>(
                &format!("/api/shixuns/{shixun_identifier}/shixun_exec.json"),
                &[],
            )
            .await?;
        valid_id(&response.game_identifier)
            .map_err(|_| Error::Response("缺少有效 game_identifier"))?;
        Ok(response.game_identifier)
    }

    pub async fn enter_shixun_for_homework(
        &self,
        shixun_identifier: &str,
        homework_id: u64,
    ) -> Result<String> {
        valid_id(shixun_identifier)?;
        if homework_id == 0 {
            return Err(Error::InvalidInput("作业 ID 无效"));
        }
        let response = self
            .get::<EnterDto>(
                &format!("/api/shixuns/{shixun_identifier}/shixun_exec.json"),
                &[("homework_common_id", homework_id.to_string())],
            )
            .await
            .map_err(write_error)?;
        valid_id(&response.game_identifier).map_err(|_| Error::WriteUncertain)?;
        Ok(response.game_identifier)
    }

    async fn wait_turn(&self) {
        let mut next = self.inner.next_request.lock().await;
        tokio::time::sleep_until(*next).await;
        *next = Instant::now() + self.inner.options.minimum_interval;
    }

    async fn server_time(&self) -> Result<i64> {
        let offset = self
            .inner
            .clock_offset
            .get_or_try_init(|| async {
                self.wait_turn().await;
                let before = now_ms()?;
                let response = self
                    .inner
                    .http
                    .head(self.inner.origin.clone())
                    .send()
                    .await
                    .map_err(Error::Transport)?;
                let after = now_ms()?;
                let date = response
                    .headers()
                    .get(header::DATE)
                    .and_then(|v| v.to_str().ok())
                    .ok_or(Error::Clock)?;
                let server_ms = httpdate::parse_http_date(date)
                    .map_err(|_| Error::Clock)?
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| Error::Clock)?
                    .as_millis();
                let server_ms = i64::try_from(server_ms).map_err(|_| Error::Clock)?;
                Ok::<_, Error>(server_ms - (before + (after - before) / 2))
            })
            .await?;
        now_ms()?.checked_add(*offset).ok_or(Error::Clock)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        self.request(Method::GET, path, query, None).await
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<T> {
        // The transport has no public arbitrary URL API: credentials stay on the fixed origin.
        let mut url = self
            .inner
            .origin
            .join(path)
            .map_err(|_| Error::InvalidInput("接口路径"))?;
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        let logical_url = Url::parse(API_ORIGIN)
            .expect("constant URL")
            .join(path)
            .map_err(|_| Error::InvalidInput("接口路径"))?;
        if self.inner.session.read().await.is_none() {
            return Err(Error::SessionExpired);
        }
        let _ = self.server_time().await?;
        self.wait_turn().await;
        let timestamp = self.server_time().await?;
        let (cookie, authorization) = self
            .inner
            .session
            .read()
            .await
            .as_ref()
            .ok_or(Error::SessionExpired)?
            .headers_for(&logical_url)?;
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, cookie);
        headers.insert("pc-authorization", authorization);
        headers.insert("x-edu-type", HeaderValue::from_static("pc"));
        headers.insert(
            "x-edu-timestamp",
            HeaderValue::from_str(&timestamp.to_string()).map_err(|_| Error::Clock)?,
        );
        headers.insert(
            "x-edu-signature",
            HeaderValue::from_str(&signing::signature(method.as_str(), timestamp))
                .map_err(|_| Error::Clock)?,
        );
        headers.insert("x-original-protocol", HeaderValue::from_static("https:"));
        headers.insert(
            "x-original-host",
            HeaderValue::from_static("www.educoder.net"),
        );
        headers.insert(
            "x-original-origin",
            HeaderValue::from_static("https://www.educoder.net"),
        );
        headers.insert(
            "x-request-id",
            HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).expect("UUID header"),
        );
        headers.insert(header::REFERER, HeaderValue::from_static(ORIGIN));
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        let mut request = self.inner.http.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.json(body);
        }
        let mut response = request.send().await.map_err(Error::Transport)?;
        let status = response.status();
        if status.as_u16() == 401
            || (status.is_redirection()
                && response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|s| s.contains("login") || s.contains("signin")))
        {
            return Err(Error::SessionExpired);
        }
        if !status.is_success() {
            return Err(Error::Http(status.as_u16()));
        }
        let limit = self.inner.options.max_response_bytes;
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(Error::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(Error::Transport)? {
            if chunk.len() > limit.saturating_sub(bytes.len()) {
                return Err(Error::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Response("预期 JSON，可能收到登录页或平台错误页"))?;
        if let Some(code) = value.get("status").and_then(Value::as_i64) {
            match code {
                -102 => return Err(Error::Clock),
                401 | -401 => return Err(Error::SessionExpired),
                code if code < 0 => return Err(Error::Platform(code)),
                _ => {}
            }
        }
        serde_json::from_value(value).map_err(|_| Error::Response("字段缺失或类型发生变化"))
    }
}

fn write_error(error: Error) -> Error {
    match error {
        Error::Transport(_) | Error::Response(_) | Error::ResponseTooLarge => Error::WriteUncertain,
        other => other,
    }
}

fn now_ms() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Clock)?
            .as_millis(),
    )
    .map_err(|_| Error::Clock)
}

fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(Error::InvalidInput(
            "标识符仅允许字母、数字、下划线和连字符",
        ));
    }
    Ok(())
}

pub(crate) fn valid_repo_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(Error::InvalidInput("必须使用仓库内相对文件路径"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
