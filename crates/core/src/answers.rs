//! Public answer archive access, isolated from the authenticated Educoder client.
use crate::{Error, Result, cache::CacheStore};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const ORIGIN: &str = "https://raw.githubusercontent.com/flyingpig233-cpu/educoder-answer/main/";
const TTL: Duration = Duration::from_secs(600);
const LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerQuery {
    pub course_id: u64,
    pub homework_id: u64,
    pub position: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Answer {
    pub title: String,
    pub status: String,
    pub editable_paths: Vec<String>,
    pub files: Vec<AnswerFile>,
}

#[derive(Debug, Serialize)]
pub struct AnswerLookup {
    pub answer: Option<Answer>,
    pub cached: bool,
    pub stale: bool,
}

#[derive(Clone, Deserialize, Serialize)]
struct Index {
    tasks: Vec<Entry>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Entry {
    course_id: u64,
    homework_id: u64,
    position: u32,
    directory: String,
    title: String,
    status: String,
    editable_paths: Vec<String>,
    answer_files: Vec<String>,
}

pub struct AnswerClient {
    http: Client,
    origin: Url,
    cache: CacheStore,
}

impl AnswerClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .user_agent("EducoderPlusPlus/answer-reader")
                .timeout(Duration::from_secs(25))
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .map_err(Error::Transport)?,
            origin: Url::parse(ORIGIN).expect("fixed answer origin"),
            cache: CacheStore::new("public-educoder-answer-v1")?,
        })
    }

    /// `None` means no indexed answer; network and malformed-index failures remain errors.
    pub async fn lookup(&self, query: &AnswerQuery, refresh: bool) -> Result<AnswerLookup> {
        let key = format!(
            "answer:{}:{}:{}",
            query.course_id, query.homework_id, query.position
        );
        let cached = self.cache.get::<Option<Answer>>(&key, TTL).await;
        if !refresh
            && let Some(value) = &cached
            && !value.stale
        {
            return Ok(AnswerLookup {
                answer: value.value.clone(),
                cached: true,
                stale: false,
            });
        }
        match self.fetch(query, refresh).await {
            Ok(answer) => {
                let _ = self.cache.put(&key, &answer).await;
                Ok(AnswerLookup {
                    answer,
                    cached: false,
                    stale: false,
                })
            }
            Err(_) if cached.as_ref().is_some_and(|c| c.value.is_some()) => Ok(AnswerLookup {
                answer: cached.unwrap().value,
                cached: true,
                stale: true,
            }),
            Err(error) => Err(error),
        }
    }

    async fn fetch(&self, query: &AnswerQuery, refresh: bool) -> Result<Option<Answer>> {
        let cached = self.cache.get::<Index>("index", TTL).await;
        let index = match cached.filter(|c| !refresh && !c.stale) {
            Some(c) => c.value,
            None => {
                let bytes = self.read("index.json").await?;
                let index: Index =
                    serde_json::from_slice(&bytes).map_err(|_| Error::Response("答案索引"))?;
                let _ = self.cache.put("index", &index).await;
                index
            }
        };
        let matches: Vec<_> = index
            .tasks
            .iter()
            .filter(|e| {
                e.course_id == query.course_id
                    && e.homework_id == query.homework_id
                    && e.position == query.position
            })
            .collect();
        if matches.len() > 1 {
            return Err(Error::Response("答案索引存在重复标识"));
        }
        let Some(entry) = matches.first() else {
            return Ok(None);
        };
        if entry.answer_files.is_empty() {
            return Ok(None);
        }
        if entry.answer_files.len() > 32 {
            return Err(Error::ResponseTooLarge);
        }
        let mut files = Vec::new();
        let mut total = 0;
        for path in &entry.answer_files {
            if !path.starts_with("answer/") {
                return Err(Error::Response("答案文件路径"));
            }
            let bytes = self.read(&format!("{}/{}", entry.directory, path)).await?;
            total += bytes.len();
            if total > LIMIT {
                return Err(Error::ResponseTooLarge);
            }
            let content =
                String::from_utf8(bytes).map_err(|_| Error::Response("答案不是 UTF-8 代码"))?;
            if !content.trim().is_empty() {
                files.push(AnswerFile {
                    path: path.clone(),
                    content,
                });
            }
        }
        if files.is_empty() {
            return Ok(None);
        }
        Ok(Some(Answer {
            title: entry.title.clone(),
            status: entry.status.clone(),
            editable_paths: entry.editable_paths.clone(),
            files,
        }))
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        if path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
            || path.contains(['\\', ':'])
            || path.chars().any(char::is_control)
        {
            return Err(Error::Response("答案仓库路径无效"));
        }
        // Encode each segment, including the archive's literal %002F escapes.
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .map_err(|_| Error::InvalidInput("答案地址"))?
            .pop_if_empty()
            .extend(path.split('/'));
        let mut response = self.http.get(url).send().await.map_err(Error::Transport)?;
        if !response.status().is_success() {
            return Err(Error::Http(response.status().as_u16()));
        }
        if response.content_length().is_some_and(|n| n > LIMIT as u64) {
            return Err(Error::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(Error::Transport)? {
            if bytes.len() + chunk.len() > LIMIT {
                return Err(Error::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    async fn setup() -> (MockServer, AnswerClient, tempfile::TempDir) {
        let server = MockServer::start().await;
        let temp = tempfile::tempdir().unwrap();
        let client = AnswerClient {
            http: Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            origin: Url::parse(&format!("{}/", server.uri())).unwrap(),
            cache: CacheStore::at(temp.path(), "test").unwrap(),
        };
        (server, client, temp)
    }
    fn query() -> AnswerQuery {
        AnswerQuery {
            course_id: 10,
            homework_id: 20,
            position: 1,
        }
    }
    fn entry() -> serde_json::Value {
        json!({"course_id":10,"homework_id":20,"position":1,"directory":"课程 [10]/01 A%002FB","title":"A/B","status":"public_samples_passed","editable_paths":["src/main.c"],"answer_files":["answer/A%002FB.c","answer/file.h"]})
    }
    async fn index(server: &MockServer, tasks: Vec<serde_json::Value>) {
        Mock::given(method("GET"))
            .and(path("/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"tasks":tasks})))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn exact_match_multifile_encoded_paths_and_offline_cache() {
        let (server, client, _temp) = setup().await;
        index(&server, vec![entry()]).await;
        for file in ["A%002FB.c", "file.h"] {
            let mut url = client.origin.clone();
            url.path_segments_mut().unwrap().pop_if_empty().extend([
                "课程 [10]",
                "01 A%002FB",
                "answer",
                file,
            ]);
            Mock::given(method("GET"))
                .and(path(url.path()))
                .respond_with(ResponseTemplate::new(200).set_body_string("int answer = 42;\n"))
                .mount(&server)
                .await;
        }
        let found = client.lookup(&query(), false).await.unwrap();
        assert!(!found.cached);
        assert_eq!(found.answer.unwrap().files.len(), 2);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        assert!(
            requests
                .iter()
                .all(|r| !r.headers.contains_key("cookie")
                    && !r.headers.contains_key("authorization"))
        );
        server.reset().await;
        assert!(client.lookup(&query(), false).await.unwrap().cached);
        let offline = client.lookup(&query(), true).await.unwrap();
        assert!(offline.stale && offline.answer.is_some());
    }

    #[tokio::test]
    async fn missing_skipped_and_errors_are_distinct() {
        let (server, client, _temp) = setup().await;
        let mut skipped = entry();
        skipped["answer_files"] = json!([]);
        index(&server, vec![skipped]).await;
        assert!(
            client
                .lookup(&query(), false)
                .await
                .unwrap()
                .answer
                .is_none()
        );
        let mut wrong = query();
        wrong.course_id = 99;
        assert!(client.lookup(&wrong, false).await.unwrap().answer.is_none());
        server.reset().await;
        assert!(client.lookup(&query(), true).await.is_err());
        index(&server, vec![entry(), entry()]).await;
        assert!(matches!(
            client.lookup(&query(), true).await,
            Err(Error::Response(_))
        ));
    }

    #[tokio::test]
    async fn untrusted_paths_never_escape_archive() {
        let (server, client, _temp) = setup().await;
        let mut unsafe_entry = entry();
        unsafe_entry["directory"] = json!("../other");
        index(&server, vec![unsafe_entry]).await;
        assert!(matches!(
            client.lookup(&query(), false).await,
            Err(Error::Response(_))
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn incomplete_or_oversized_archive_is_an_error_not_a_missing_answer() {
        let (server, client, _temp) = setup().await;
        let mut simple = entry();
        simple["directory"] = json!("demo");
        simple["answer_files"] = json!(["answer/main.c"]);
        index(&server, vec![simple.clone()]).await;
        assert!(matches!(
            client.lookup(&query(), true).await,
            Err(Error::Http(404))
        ));
        server.reset().await;
        index(&server, vec![simple]).await;
        Mock::given(path("/demo/answer/main.c"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; LIMIT + 1]))
            .mount(&server)
            .await;
        assert!(matches!(
            client.lookup(&query(), true).await,
            Err(Error::ResponseTooLarge)
        ));
    }
}
