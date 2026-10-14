//! Read-only live checks. Prints counts only, never account details or code.
use educoder_core::{
    ClientOptions, EducoderClient, Error, Session,
    models::{HomeworkKind, PageRequest},
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("FAIL: {error}");
        if let Some(Error::Transport(transport)) = error.downcast_ref::<Error>() {
            eprintln!(
                "NETWORK: timeout={}; connect={}",
                transport.is_timeout(),
                transport.is_connect()
            );
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("EDUCODER_COOKIES")
        .ok_or("Set EDUCODER_COOKIES to your cookies.txt file")?;
    let timeout_seconds = std::env::var("EDUCODER_TIMEOUT_SECONDS")
        .ok()
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(30);
    let client = EducoderClient::with_options(
        Session::from_file(path).await?,
        ClientOptions {
            timeout: std::time::Duration::from_secs(timeout_seconds),
            ..Default::default()
        },
    )?;
    client.validate_session().await?;
    println!("PASS: session validation");
    let courses = client.get_courses().await?;
    println!(
        "PASS: courses; returned={}; total={:?}",
        courses.items.len(),
        courses.total
    );
    let Some(course) = courses.items.first() else {
        println!("SKIP: homework/task checks (no accessible courses)");
        return Ok(());
    };
    let mut candidate = None;
    for (kind, label) in [
        (HomeworkKind::Shixun, "shixun"),
        (HomeworkKind::Text, "text"),
    ] {
        let page = client
            .get_homeworks(course.id, kind, PageRequest { page: 1, limit: 20 })
            .await?;
        println!(
            "PASS: {label} homeworks page=1; returned={}; total={:?}",
            page.items.len(),
            page.total
        );
        if matches!(kind, HomeworkKind::Shixun) {
            candidate = page
                .items
                .iter()
                .find(|h| h.shixun_identifier.is_some() && h.myshixun_identifier.is_some())
                .cloned();
        }
        let page = client
            .get_homeworks(course.id, kind, PageRequest { page: 2, limit: 20 })
            .await?;
        println!(
            "PASS: {label} homeworks page=2; returned={}",
            page.items.len()
        );
    }
    let Some(homework) = candidate else {
        println!(
            "SKIP: task/code/report checks (no existing shixun instance in sampled homeworks)"
        );
        return Ok(());
    };
    let shixun_id = homework
        .shixun_identifier
        .as_deref()
        .ok_or("Missing shixun identifier")?;
    client.get_shixun(shixun_id).await?;
    println!("PASS: shixun details (read only)");
    let challenges = client.get_challenges(shixun_id).await?;
    println!("PASS: challenges; returned={}", challenges.len());
    let instance = homework
        .myshixun_identifier
        .as_deref()
        .ok_or("Missing existing instance identifier")?;
    let games = client.get_my_challenges(instance).await?;
    println!("PASS: existing instance games; returned={}", games.len());
    if let Some(game) = games.first() {
        let details = client
            .get_task_details(&game.identifier, homework.id)
            .await?;
        println!(
            "PASS: task evaluation details; tests={}; public={}; submission_context={}",
            details.evaluation.tests.len(),
            details.evaluation.tests.iter().filter(|t| t.public).count(),
            details.submission.is_some()
        );
        let snapshot = client.get_task_snapshot(&game.identifier).await?;
        println!(
            "PASS: task snapshot; description_bytes={}; files={}; code_bytes={}",
            snapshot.task.description_markdown.len(),
            snapshot.files.len(),
            snapshot
                .files
                .iter()
                .map(|f| f.content.len())
                .sum::<usize>()
        );
    } else {
        println!("SKIP: task/code checks (no existing games)");
    }
    if let Some(report_id) = homework.student_work_id {
        let report = client.get_work_report(report_id).await?;
        println!("PASS: work report; stages={}", report.stages.len());
    } else {
        println!("SKIP: report check (no report ID)");
    }
    Ok(())
}
