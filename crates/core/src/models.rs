//! Domain data exposed to the GUI. Platform JSON details are private to the crate.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Course {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Homework {
    pub id: u64,
    pub name: String,
    pub shixun_identifier: Option<String>,
    pub myshixun_identifier: Option<String>,
    pub student_work_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct Challenge {
    pub position: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Game {
    pub identifier: String,
    pub position: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Shixun {
    pub name: String,
    /// Numeric instance ID; this is distinct from its string identifier.
    pub myshixun_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub game_identifier: String,
    pub title: String,
    pub position: u32,
    pub description_markdown: String,
    pub editable_paths: Vec<String>,
    pub shixun_name: Option<String>,
    pub myshixun_identifier: Option<String>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub task: Task,
    pub files: Vec<CodeFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportStage {
    pub name: String,
    pub score: Option<f64>,
    pub finished_time: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkReport {
    pub homework_name: Option<String>,
    pub course_name: Option<String>,
    pub score: Option<f64>,
    pub stages: Vec<ReportStage>,
}
