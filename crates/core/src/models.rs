//! Domain data exposed to the GUI. Platform JSON details are private to the crate.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub login: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Homework {
    pub id: u64,
    pub name: String,
    pub shixun_identifier: Option<String>,
    pub myshixun_identifier: Option<String>,
    pub student_work_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// None means the server did not supply a total, not zero items.
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct PageRequest {
    pub page: u32,
    pub limit: u32,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self { page: 1, limit: 50 }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HomeworkKind {
    Text,
    Shixun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub position: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub identifier: String,
    pub position: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shixun {
    pub name: String,
    /// Numeric instance ID; this is distinct from its string identifier.
    pub myshixun_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub game_identifier: String,
    pub title: String,
    pub position: u32,
    pub description_markdown: String,
    pub editable_paths: Vec<String>,
    pub shixun_name: Option<String>,
    pub myshixun_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFile {
    pub path: String,
    /// Preserve bytes: not every repository file is valid UTF-8.
    pub content: Vec<u8>,
}

impl CodeFile {
    pub fn text(&self) -> std::result::Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.content)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub task: Task,
    pub files: Vec<CodeFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportStage {
    pub name: String,
    pub score: Option<f64>,
    pub finished_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkReport {
    pub homework_name: Option<String>,
    pub course_name: Option<String>,
    pub score: Option<f64>,
    pub stages: Vec<ReportStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestCase {
    pub name: String,
    pub input: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub public: bool,
    pub passed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evaluation {
    pub status: i32,
    pub revision: Option<u64>,
    pub compile_output: String,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionContext {
    pub game_id: u64,
    pub challenge_id: u64,
    pub user_id: u64,
    pub homework_id: u64,
    pub myshixun_identifier: String,
    pub environment_id: u64,
    pub tab_type: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetails {
    pub task: Task,
    pub evaluation: Evaluation,
    pub submission: Option<SubmissionContext>,
}

/// A complete editor snapshot, shared by foreground loading and background prefetch.
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskDocument {
    pub details: TaskDetails,
    pub files: Vec<CodeFile>,
}

/// Temporary save receipt. Security keys never enter UI messages or persistent caches.
pub struct SaveReceipt {
    pub(crate) commit_id: String,
    pub(crate) sec_key: String,
    pub(crate) resubmit: serde_json::Value,
    pub(crate) content_modified: serde_json::Value,
}

impl std::fmt::Debug for SaveReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SaveReceipt([REDACTED])")
    }
}
