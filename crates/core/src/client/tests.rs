use super::*;
use serde_json::json;
use wiremock::{
    Match, Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path, query_param},
};

async fn client() -> (MockServer, EducoderClient) {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("date", httpdate::fmt_http_date(SystemTime::now())),
        )
        .mount(&server)
        .await;
    let client = EducoderClient {
        inner: Arc::new(Inner {
            http: Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .unwrap(),
            origin: Url::parse(&server.uri()).unwrap(),
            session: RwLock::new(Some(
                Session::from_cookie_header("_educoder_session=test-session").unwrap(),
            )),
            clock_offset: OnceCell::new(),
            next_request: Mutex::new(Instant::now()),
            options: ClientOptions {
                minimum_interval: Duration::ZERO,
                ..Default::default()
            },
        }),
    };
    (server, client)
}

struct Signed;
impl Match for Signed {
    fn matches(&self, request: &Request) -> bool {
        let timestamp = request
            .headers
            .get("x-edu-timestamp")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok());
        let signature = request
            .headers
            .get("x-edu-signature")
            .and_then(|v| v.to_str().ok());
        timestamp.is_some_and(|t| {
            signature == Some(signing::signature(request.method.as_str(), t).as_str())
        }) && request
            .headers
            .get("pc-authorization")
            .is_some_and(|v| v == "test-session")
            && request
                .headers
                .get("cookie")
                .is_some_and(|v| v == "_educoder_session=test-session")
    }
}

#[tokio::test]
async fn avatars_use_platform_paths_without_credentials_or_redirects() {
    let (server, client) = client().await;
    let mut user: User = serde_json::from_value::<UserDto>(json!({
        "login":"demo", "image_url":"avatars/User/demo-photo.jpg"
    }))
    .unwrap()
    .into();
    Mock::given(method("GET"))
        .and(path("/images/avatars/User/demo-photo.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1, 2, 3]))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client.get_user_avatar(&user).await.unwrap(),
        Some(vec![1, 2, 3])
    );
    let requests = server.received_requests().await.unwrap();
    let request = requests
        .iter()
        .find(|r| r.url.path().contains("avatars"))
        .unwrap();
    assert!(!request.headers.contains_key("cookie"));
    assert!(!request.headers.contains_key("pc-authorization"));
    for path in [
        "https://example.org/avatar",
        "avatars/User/../private",
        "avatars/Admin/demo",
    ] {
        user.avatar_path = Some(path.into());
        assert!(client.get_user_avatar(&user).await.is_err());
    }
    user.avatar_path = Some("avatars/User/redirect".into());
    Mock::given(path("/images/avatars/User/redirect"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/unexpected", server.uri())),
        )
        .mount(&server)
        .await;
    assert!(matches!(
        client.get_user_avatar(&user).await,
        Err(Error::Http(302))
    ));
    user.avatar_path = None;
    assert!(client.get_user_avatar(&user).await.unwrap().is_none());
    let old_cache: User =
        serde_json::from_value(json!({"login":"demo","display_name":"Demo"})).unwrap();
    assert!(old_cache.avatar_path.is_none());
}

#[tokio::test]
async fn signs_requests_and_maps_user_and_course_variants() {
    let (server, client) = client().await;
    Mock::given(method("GET"))
        .and(path("/api/users/get_user_info.json"))
        .and(Signed)
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"login":"student", "real_name":"Demo", "unexpected":"ignored"}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/users/student/courses.json"))
        .and(Signed)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"courses":[{"id":7,"name":"Rust"}], "count":1})),
        )
        .mount(&server)
        .await;
    assert_eq!(
        client.validate_session().await.unwrap().display_name,
        "Demo"
    );
    let courses = client.get_courses().await.unwrap();
    assert_eq!(courses.total, Some(1));
    assert_eq!(courses.items[0].id, 7);
    let list: Page<Course> = serde_json::from_value::<CoursesDto>(json!([{ "id":8, "name":"C" }]))
        .unwrap()
        .into();
    assert_eq!(list.total, None);
    assert_eq!(list.items[0].name, "C");
}

#[tokio::test]
async fn homeworks_use_explicit_pagination_and_kind() {
    let (server, client) = client().await;
    Mock::given(path("/api/courses/7/homework_commons.json"))
        .and(query_param("type", "4")).and(query_param("page", "2")).and(query_param("limit", "20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"homeworks":[{"homework_id":9,"name":"Demo", "shixun_identifier":"sh1", "myshixun_identifier":"", "student_work_id":0}],"query_total_count":21,"all_count":99})))
        .expect(1).mount(&server).await;
    let page = client
        .get_homeworks(7, HomeworkKind::Shixun, PageRequest { page: 2, limit: 20 })
        .await
        .unwrap();
    assert_eq!(page.total, Some(21));
    assert!(page.items[0].myshixun_identifier.is_none());
    assert!(page.items[0].student_work_id.is_none());
    assert!(
        client
            .get_homeworks(7, HomeworkKind::Text, PageRequest { page: 0, limit: 20 })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn snapshot_decodes_paths_and_does_not_enter_instances() {
    let (server, client) = client().await;
    Mock::given(path("/api/tasks/g1.json")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "challenge":{"subject":"Hello", "position":1,"task_pass":"# Task", "path":"src/中文 +&.rs；data.bin;src/中文 +&.rs;"},
        "shixun":{"name":"Rust"}, "myshixun":{"identifier":"mine"}
    }))).expect(1).mount(&server).await;
    Mock::given(path("/api/tasks/g1/rep_content.json"))
        .and(query_param("path", "src/中文 +&.rs"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"content":{"content":STANDARD.encode("fn main() {}\n")}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/tasks/g1/rep_content.json"))
        .and(query_param("path", "data.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"content":"/wA="})))
        .expect(1)
        .mount(&server)
        .await;
    let snapshot = client.get_task_snapshot("g1").await.unwrap();
    assert_eq!(snapshot.task.description_markdown, "# Task");
    assert_eq!(snapshot.files.len(), 2);
    assert_eq!(snapshot.files[0].text().unwrap(), "fn main() {}\n");
    assert_eq!(snapshot.files[1].content, [255, 0]);
    assert!(snapshot.files[1].text().is_err());
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|r| !r.url.path().contains("shixun_exec"))
    );
}

#[tokio::test]
async fn exposes_known_shixun_game_and_report_models() {
    let (server, client) = client().await;
    for (url, body) in [
        (
            "/api/shixuns/sh1.json",
            json!({"name":"Rust", "myshixun_id":0}),
        ),
        (
            "/api/shixuns/sh1/challenges.json",
            json!({"challenge_list":[{"name":"A","position":1}]}),
        ),
        (
            "/api/myshixuns/12/challenges.json",
            json!([{"name":"A","position":1,"identifier":"g1"}]),
        ),
        (
            "/api/student_works/9/shixun_work_report.json",
            json!({"work_score":99.5, "stage_list":[{"name":"A", "game_score":50, "finished_time":"--"}]}),
        ),
        (
            "/api/shixuns/sh1/shixun_exec.json",
            json!({"game_identifier":"g1"}),
        ),
    ] {
        Mock::given(path(url))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
    }
    assert!(
        client
            .get_shixun("sh1")
            .await
            .unwrap()
            .myshixun_id
            .is_none()
    );
    assert_eq!(client.get_challenges("sh1").await.unwrap()[0].position, 1);
    assert_eq!(
        client.get_my_challenges("12").await.unwrap()[0].identifier,
        "g1"
    );
    assert_eq!(client.get_work_report(9).await.unwrap().score, Some(99.5));
    assert_eq!(client.enter_shixun("sh1").await.unwrap(), "g1");
}

#[tokio::test]
async fn redirects_never_forward_credentials_and_expiry_is_distinct() {
    let (server, client) = client().await;
    let other = MockServer::start().await;
    Mock::given(path("/api/tasks/g1.json"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/login", other.uri())),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client.get_task("g1").await,
        Err(Error::SessionExpired)
    ));
    assert!(other.received_requests().await.unwrap().is_empty());
    Mock::given(path("/api/tasks/g2.json"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client.get_task("g2").await,
        Err(Error::SessionExpired)
    ));
    let clone = client.clone();
    client.logout(None).await.unwrap();
    assert!(matches!(
        clone.get_task("g2").await,
        Err(Error::SessionExpired)
    ));
}

#[tokio::test]
async fn detects_platform_errors_schema_changes_and_invalid_code() {
    let (server, client) = client().await;
    for (id, body) in [
        ("clock", json!({"status":-102})),
        ("denied", json!({"status":-1, "message":"sensitive"})),
        ("schema", json!({"challenge":{}})),
    ] {
        Mock::given(path(format!("/api/tasks/{id}.json")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    assert!(matches!(client.get_task("clock").await, Err(Error::Clock)));
    let error = client.get_task("denied").await.unwrap_err();
    assert!(matches!(error, Error::Platform(-1)));
    assert!(!format!("{error:?}").contains("sensitive"));
    assert!(matches!(
        client.get_task("schema").await,
        Err(Error::Response(_))
    ));
    Mock::given(path("/api/tasks/g1/rep_content.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"content":"***"})))
        .mount(&server)
        .await;
    assert!(matches!(
        client.get_file_content("g1", "src/a.rs").await,
        Err(Error::Response(_))
    ));
    for name in ["../a", "/tmp/a", "C:/a", "a\\b", "a/../b", "a\n"] {
        assert!(matches!(
            client.get_file_content("g1", name).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        client.get_task("../g1").await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn bounds_responses_and_does_not_retry_failures() {
    let (server, mut client) = client().await;
    Arc::get_mut(&mut client.inner)
        .unwrap()
        .options
        .max_response_bytes = 16;
    Mock::given(path("/api/tasks/big.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("a".repeat(32)))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client.get_task("big").await,
        Err(Error::ResponseTooLarge)
    ));
    Mock::given(path("/api/shixuns/sh1/shixun_exec.json"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client.enter_shixun("sh1").await,
        Err(Error::Http(503))
    ));
}

#[tokio::test]
async fn calibrates_clock_once_and_spaces_concurrent_requests() {
    let (server, mut client) = client().await;
    Arc::get_mut(&mut client.inner)
        .unwrap()
        .options
        .minimum_interval = Duration::from_millis(30);
    Mock::given(path("/api/users/get_user_info.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"demo"})))
        .expect(2)
        .mount(&server)
        .await;
    let started = Instant::now();
    let (a, b) = tokio::join!(client.get_user_info(), client.get_user_info());
    assert!(a.is_ok() && b.is_ok());
    assert!(started.elapsed() >= Duration::from_millis(60));
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "HEAD")
            .count(),
        1
    );
}

fn task_fixture(status: i32) -> Value {
    json!({"challenge":{"id":31,"subject":"Demo","position":1,"task_pass":"# Demo","path":"main.rs;helper.rs"},
        "game":{"id":71,"status":status,"user_id":21},"user":{"user_id":21},"myshixun":{"identifier":"instance"},
        "shixun_environments":[{"shixun_environment_id":11,"tab_type":1}],"homework_common_id":9,"evaluate_count":4,
        "test_sets":[{"name":"public","is_public":true,"input":"1 2","output":"3","actual_output":"3","result":true},
            {"name":"private","is_public":false,"input":"never disclose","output":"never disclose","actual_output":null,"result":null}],"last_compile_output":"compile successfully"})
}

#[tokio::test]
async fn save_all_files_then_submit_once_with_signed_post_and_private_tests_redacted() {
    use wiremock::matchers::body_partial_json;
    let (server, client) = client().await;
    Mock::given(path("/api/tasks/g1.json"))
        .and(query_param("homework_common_id", "9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_fixture(0)))
        .expect(1)
        .mount(&server)
        .await;
    let task = client.get_task_details("g1", 9).await.unwrap();
    assert!(task.submission.is_some());
    assert_eq!(task.evaluation.tests[0].input.as_deref(), Some("1 2"));
    assert!(
        task.evaluation.tests[1].input.is_none() && task.evaluation.tests[1].expected.is_none()
    );
    for (path_name, evaluate, key) in [
        ("main.rs", 0, Value::Null),
        ("helper.rs", 1, json!("temporary-key")),
    ] {
        Mock::given(method("POST")).and(path("/api/myshixuns/instance/update_file.json")).and(Signed)
            .and(body_partial_json(json!({"path":path_name,"evaluate":evaluate,"game_id":71,"homework_common_id":9,"extras":{"challenge_id":31,"currentUserId":21}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"content":{"commitID":"commit"},"sec_key":key,"resubmit":"","content_modified":0}))).expect(1).mount(&server).await;
    }
    Mock::given(method("POST")).and(path("/api/tasks/g1/game_build.json")).and(Signed)
        .and(body_partial_json(json!({"sec_key":"temporary-key","shixun_environment_id":11,"first":1,"extras":{"commitID":"commit","homework_common_id":9}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":1}))).expect(1).mount(&server).await;
    client
        .submit_code(
            &task,
            &[
                CodeFile {
                    path: "main.rs".into(),
                    content: b"fn main() {}".to_vec(),
                },
                CodeFile {
                    path: "helper.rs".into(),
                    content: b"// helper".to_vec(),
                },
            ],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_batch_cannot_partially_save_and_old_pass_cannot_complete_new_evaluation() {
    let (server, client) = client().await;
    let task = serde_json::from_value::<TaskDetailsDto>(task_fixture(2))
        .unwrap()
        .into_details("g1", 9);
    let files = [
        CodeFile {
            path: "main.rs".into(),
            content: vec![],
        },
        CodeFile {
            path: "outside.rs".into(),
            content: vec![],
        },
    ];
    assert!(matches!(
        client.save_files(&task, &files, false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
    Mock::given(path("/api/tasks/g1.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_fixture(2)))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client
            .wait_for_evaluation(&task, Duration::from_millis(2100))
            .await,
        Err(Error::EvaluationPending)
    ));
}

#[tokio::test]
async fn ambiguous_write_is_not_retried_and_security_receipt_debug_is_redacted() {
    let (server, mut client) = client().await;
    Arc::get_mut(&mut client.inner).unwrap().http = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(80))
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let task = serde_json::from_value::<TaskDetailsDto>(task_fixture(0))
        .unwrap()
        .into_details("g1", 9);
    Mock::given(method("POST"))
        .and(path("/api/myshixuns/instance/update_file.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(1))
                .set_body_json(json!({"content":{"commitID":"x"},"sec_key":"secret"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client
            .save_files(
                &task,
                &[CodeFile {
                    path: "main.rs".into(),
                    content: vec![]
                }],
                true
            )
            .await,
        Err(Error::WriteUncertain)
    ));
    let receipt =
        serde_json::from_value::<SaveDto>(json!({"content":{"commitID":"x"},"sec_key":"secret"}))
            .unwrap()
            .into_receipt();
    assert!(!format!("{receipt:?}").contains("secret"));
}
