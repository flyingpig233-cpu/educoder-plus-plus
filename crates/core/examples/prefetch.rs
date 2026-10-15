//! Read-only cache warm-up check. Counts only; never saves or evaluates platform code.
use educoder_core::{
    EducoderClient, Session,
    cache::CacheStore,
    models::{HomeworkKind, PageRequest, TaskDocument},
    workspace::WorkspaceCache,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Cache prefetch failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("EDUCODER_COOKIES").ok_or("Set EDUCODER_COOKIES")?;
    let session = Session::from_file(path).await?;
    let store = CacheStore::new(&session.cache_namespace()?)?;
    let client = EducoderClient::new(session)?;
    let workspace = WorkspaceCache::new(client.clone(), store.clone());
    let courses = client.get_courses().await?;
    let Some(course) = courses.items.first() else {
        return Ok(());
    };
    let count = std::env::args()
        .nth(1)
        .map(|a| a.parse::<usize>())
        .transpose()?
        .unwrap_or(1)
        .min(20);
    let page = client
        .get_homeworks(
            course.id,
            HomeworkKind::Shixun,
            PageRequest { page: 1, limit: 20 },
        )
        .await?;
    let mut tasks = 0;
    let mut files = 0;
    let mut hits = 0;
    for homework in page.items.iter().take(count) {
        for game in workspace.games(homework, false).await? {
            if store
                .get::<TaskDocument>(
                    &format!("task:{}:{}", homework.id, game.identifier),
                    Duration::from_secs(600),
                )
                .await
                .is_some_and(|c| !c.stale)
            {
                hits += 1;
            }
            let document = workspace
                .document(homework.id, &game.identifier, false)
                .await?;
            tasks += 1;
            files += document.files.len();
        }
        println!("prefetched tasks={tasks}; files={files}; cache_hits={hits}");
    }
    Ok(())
}
