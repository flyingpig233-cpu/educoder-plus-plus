use crate::models::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct UserDto {
    login: String,
    username: Option<String>,
    real_name: Option<String>,
    image_url: Option<String>,
}
impl From<UserDto> for User {
    fn from(v: UserDto) -> Self {
        Self {
            display_name: v
                .real_name
                .filter(|s| !s.is_empty())
                .or(v.username)
                .unwrap_or_else(|| v.login.clone()),
            login: v.login,
            avatar_path: v.image_url.filter(|path| !path.is_empty()),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CourseDto {
    id: u64,
    name: String,
}
impl From<CourseDto> for Course {
    fn from(v: CourseDto) -> Self {
        Self {
            id: v.id,
            name: v.name,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum CoursesDto {
    List(Vec<CourseDto>),
    Page {
        courses: Vec<CourseDto>,
        count: Option<u64>,
    },
}
impl From<CoursesDto> for Page<Course> {
    fn from(v: CoursesDto) -> Self {
        let (items, total) = match v {
            CoursesDto::List(items) => (items, None),
            CoursesDto::Page { courses, count } => (courses, count),
        };
        Self {
            items: items.into_iter().map(Into::into).collect(),
            total,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct HomeworkDto {
    #[serde(rename = "homework_id", alias = "id")]
    id: u64,
    name: String,
    shixun_identifier: Option<String>,
    myshixun_identifier: Option<String>,
    student_work_id: Option<u64>,
}
#[derive(Deserialize)]
pub(crate) struct HomeworksDto {
    homeworks: Vec<HomeworkDto>,
    #[serde(rename = "query_total_count", alias = "count")]
    count: Option<u64>,
}
impl From<HomeworksDto> for Page<Homework> {
    fn from(v: HomeworksDto) -> Self {
        Self {
            total: v.count,
            items: v
                .homeworks
                .into_iter()
                .map(|h| Homework {
                    id: h.id,
                    name: h.name,
                    shixun_identifier: h.shixun_identifier.filter(|s| !s.is_empty()),
                    myshixun_identifier: h.myshixun_identifier.filter(|s| !s.is_empty()),
                    student_work_id: h.student_work_id.filter(|id| *id != 0),
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ChallengeDto {
    position: u32,
    name: String,
}
#[derive(Deserialize)]
pub(crate) struct ChallengesDto {
    pub challenge_list: Vec<ChallengeDto>,
}
impl From<ChallengeDto> for Challenge {
    fn from(v: ChallengeDto) -> Self {
        Self {
            position: v.position,
            name: v.name,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct GameDto {
    identifier: String,
    position: u32,
    name: String,
}
impl From<GameDto> for Game {
    fn from(v: GameDto) -> Self {
        Self {
            identifier: v.identifier,
            position: v.position,
            name: v.name,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ShixunDto {
    name: String,
    myshixun_id: Option<u64>,
}
impl From<ShixunDto> for Shixun {
    fn from(v: ShixunDto) -> Self {
        Self {
            name: v.name,
            myshixun_id: v.myshixun_id.filter(|id| *id != 0),
        }
    }
}

#[derive(Deserialize)]
struct TaskChallengeDto {
    id: Option<u64>,
    subject: String,
    position: u32,
    task_pass: Option<String>,
    path: Option<String>,
}
#[derive(Deserialize)]
struct NameDto {
    name: String,
}
#[derive(Deserialize)]
struct InstanceDto {
    identifier: String,
}

#[derive(Deserialize)]
struct EvaluationGameDto {
    id: Option<u64>,
    status: i32,
    user_id: Option<u64>,
    evaluate_count: Option<u64>,
}
#[derive(Deserialize)]
struct EvaluationUserDto {
    user_id: Option<u64>,
}
#[derive(Deserialize)]
struct EnvironmentDto {
    shixun_environment_id: u64,
    tab_type: Option<u32>,
}
#[derive(Deserialize)]
struct TestCaseDto {
    name: Option<String>,
    input: Option<String>,
    output: Option<String>,
    actual_output: Option<String>,
    #[serde(default)]
    is_public: bool,
    result: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct EvaluationResultDto {
    pub status: i32,
    #[serde(default)]
    test_sets: Vec<TestCaseDto>,
    last_compile_output: Option<String>,
    evaluate_count: Option<u64>,
}
impl EvaluationResultDto {
    pub fn into_evaluation(self) -> Evaluation {
        Evaluation {
            status: if self.status == 2 { 2 } else { 3 },
            revision: self.evaluate_count,
            compile_output: self.last_compile_output.unwrap_or_default(),
            tests: self
                .test_sets
                .into_iter()
                .enumerate()
                .map(|(index, t)| TestCase {
                    name: t.name.unwrap_or_else(|| format!("测试集 {}", index + 1)),
                    input: t.is_public.then_some(t.input).flatten(),
                    expected: t.is_public.then_some(t.output).flatten(),
                    actual: t.actual_output,
                    public: t.is_public,
                    passed: t.result,
                })
                .collect(),
        }
    }
}
#[derive(Deserialize)]
pub(crate) struct TaskDetailsDto {
    #[serde(flatten)]
    task: TaskDto,
    game: EvaluationGameDto,
    user: Option<EvaluationUserDto>,
    homework_common_id: Option<u64>,
    #[serde(default)]
    shixun_environments: Vec<EnvironmentDto>,
    #[serde(default)]
    test_sets: Vec<TestCaseDto>,
    last_compile_output: Option<String>,
    evaluate_count: Option<u64>,
}

impl TaskDetailsDto {
    pub fn into_details(self, game: &str, homework_id: u64) -> TaskDetails {
        let user_id = self.user.and_then(|u| u.user_id).or(self.game.user_id);
        let submission = match (
            self.game.id,
            self.task.challenge.id,
            user_id,
            self.task.myshixun.as_ref(),
            self.shixun_environments.first(),
        ) {
            (Some(game_id), Some(challenge_id), Some(user_id), Some(instance), Some(env)) => {
                Some(SubmissionContext {
                    game_id,
                    challenge_id,
                    user_id,
                    homework_id: self
                        .homework_common_id
                        .filter(|id| *id > 0)
                        .unwrap_or(homework_id),
                    myshixun_identifier: instance.identifier.clone(),
                    environment_id: env.shixun_environment_id,
                    tab_type: env.tab_type.unwrap_or(1),
                })
            }
            _ => None,
        };
        TaskDetails {
            task: self.task.into_task(game),
            submission,
            evaluation: Evaluation {
                status: self.game.status,
                revision: self.evaluate_count.or(self.game.evaluate_count),
                compile_output: self.last_compile_output.unwrap_or_default(),
                tests: self
                    .test_sets
                    .into_iter()
                    .enumerate()
                    .map(|(index, t)| TestCase {
                        name: t.name.unwrap_or_else(|| format!("测试集 {}", index + 1)),
                        // Never display or execute private test inputs/expected outputs.
                        input: t.is_public.then_some(t.input).flatten(),
                        expected: t.is_public.then_some(t.output).flatten(),
                        actual: t.actual_output,
                        public: t.is_public,
                        passed: t.result,
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct SaveDto {
    content: SaveContentDto,
    sec_key: Option<String>,
    #[serde(default)]
    resubmit: serde_json::Value,
    #[serde(default)]
    content_modified: serde_json::Value,
}
#[derive(Deserialize)]
struct SaveContentDto {
    #[serde(rename = "commitID")]
    commit_id: String,
}
impl SaveDto {
    pub fn into_receipt(self) -> SaveReceipt {
        SaveReceipt {
            commit_id: self.content.commit_id,
            sec_key: self.sec_key.unwrap_or_default(),
            resubmit: self.resubmit,
            content_modified: self.content_modified,
        }
    }
}
#[derive(Deserialize)]
pub(crate) struct TaskDto {
    challenge: TaskChallengeDto,
    shixun: Option<NameDto>,
    myshixun: Option<InstanceDto>,
}
impl TaskDto {
    pub fn into_task(self, game_identifier: &str) -> Task {
        Task {
            game_identifier: game_identifier.into(),
            title: self.challenge.subject,
            position: self.challenge.position,
            description_markdown: self.challenge.task_pass.unwrap_or_default(),
            editable_paths: self
                .challenge
                .path
                .unwrap_or_default()
                .split([';', '；'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            shixun_name: self.shixun.map(|s| s.name),
            myshixun_identifier: self.myshixun.map(|s| s.identifier),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ContentDto {
    Direct(String),
    Nested { content: String },
}
#[derive(Deserialize)]
pub(crate) struct FileDto {
    content: ContentDto,
}
impl FileDto {
    pub fn encoded(self) -> String {
        match self.content {
            ContentDto::Direct(s) | ContentDto::Nested { content: s } => s,
        }
    }
}

#[derive(Deserialize)]
struct StageDto {
    name: String,
    #[serde(default, deserialize_with = "optional_score")]
    game_score: Option<f64>,
    finished_time: Option<String>,
}
#[derive(Deserialize)]
pub(crate) struct ReportDto {
    homework_name: Option<String>,
    course_name: Option<String>,
    #[serde(default, deserialize_with = "optional_score")]
    work_score: Option<f64>,
    stage_list: Vec<StageDto>,
}
impl From<ReportDto> for WorkReport {
    fn from(v: ReportDto) -> Self {
        Self {
            homework_name: v.homework_name,
            course_name: v.course_name,
            score: v.work_score,
            stages: v
                .stage_list
                .into_iter()
                .map(|s| ReportStage {
                    name: s.name,
                    score: s.game_score,
                    finished_time: s.finished_time,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct EnterDto {
    pub game_identifier: String,
}

fn optional_score<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Score {
        Number(f64),
        Text(String),
    }
    let value = match Option::<Score>::deserialize(deserializer)? {
        None => return Ok(None),
        Some(Score::Number(value)) => value,
        Some(Score::Text(text)) => match text.trim() {
            "" | "--" | "-" => return Ok(None),
            number => number.parse::<f64>().map_err(|_| {
                serde::de::Error::custom("expected numeric score or ungraded marker")
            })?,
        },
    };
    if !value.is_finite() {
        return Err(serde::de::Error::custom("expected finite score"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn report_scores_accept_platform_strings_and_preserve_ungraded_values() {
        let report: WorkReport = serde_json::from_value::<ReportDto>(json!({
            "work_score":"88.5", "stage_list":[
                {"name":"A", "game_score":"20.0"},
                {"name":"B", "game_score":"--"},
                {"name":"C", "game_score":0},
                {"name":"D", "game_score":null},
                {"name":"E"}
            ]
        }))
        .unwrap()
        .into();
        assert_eq!(report.score, Some(88.5));
        assert_eq!(
            report.stages.iter().map(|s| s.score).collect::<Vec<_>>(),
            vec![Some(20.0), None, Some(0.0), None, None]
        );
        for invalid in ["not a score", "NaN", "inf"] {
            assert!(
                serde_json::from_value::<ReportDto>(
                    json!({"work_score": invalid, "stage_list":[]})
                )
                .is_err()
            );
        }
    }
}
