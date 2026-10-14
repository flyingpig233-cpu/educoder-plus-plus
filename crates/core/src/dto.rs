use crate::models::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct UserDto {
    login: String,
    username: Option<String>,
    real_name: Option<String>,
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
