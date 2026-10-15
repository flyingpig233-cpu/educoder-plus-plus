//! Read-only workspace prefetch, shared with interactive loading.
use crate::{
    EducoderClient, Error, Result,
    cache::CacheStore,
    models::{Game, Homework, HomeworkKind, Page, PageRequest, TaskDocument},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Mutex;

const TTL: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub struct WorkspaceCache {
    client: EducoderClient,
    store: CacheStore,
    flights: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    generation: Arc<AtomicU64>,
    writes: Arc<Mutex<()>>,
}
impl WorkspaceCache {
    pub async fn homework_page(&self, course: u64, page: u32) -> Result<Page<Homework>> {
        let key = format!("homeworks:{course}:0:{page}");
        let _guard = self.lock(&key).await;
        if let Some(cached) = self.store.get::<Page<Homework>>(&key, TTL).await
            && !cached.stale
        {
            return Ok(cached.value);
        }
        let result = self
            .client
            .get_homeworks(
                course,
                HomeworkKind::Shixun,
                PageRequest { page, limit: 20 },
            )
            .await?;
        self.store.put(&key, &result).await?;
        Ok(result)
    }
    pub fn new(client: EducoderClient, store: CacheStore) -> Self {
        Self {
            client,
            store,
            flights: Default::default(),
            generation: Default::default(),
            writes: Default::default(),
        }
    }
    async fn lock(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let flight = self
            .flights
            .lock()
            .await
            .entry(key.to_owned())
            .or_default()
            .clone();
        flight.lock_owned().await
    }
    /// Prevent an in-flight old snapshot from replacing the cache after an editor write.
    pub async fn invalidate_reads(&self) {
        let _guard = self.writes.lock().await;
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub async fn games(&self, homework: &Homework, force: bool) -> Result<Vec<Game>> {
        let key = format!("games:{}", homework.id);
        let _guard = self.lock(&key).await;
        if !force
            && let Some(cached) = self.store.get::<Vec<Game>>(&key, TTL).await
            && !cached.stale
        {
            return Ok(cached.value);
        }
        let games = if let Some(instance) = &homework.myshixun_identifier {
            self.client.get_my_challenges(instance).await?
        } else if let Some(shixun) = &homework.shixun_identifier {
            // get_shixun only inspects an existing instance; never call shixun_exec here.
            if let Some(id) = self.client.get_shixun(shixun).await?.myshixun_id {
                self.client.get_my_challenges(&id.to_string()).await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        self.store.put(&key, &games).await?;
        Ok(games)
    }

    pub async fn document(
        &self,
        homework_id: u64,
        game: &str,
        force: bool,
    ) -> Result<TaskDocument> {
        let key = format!("task:{homework_id}:{game}");
        let _guard = self.lock(&key).await;
        if !force
            && let Some(cached) = self.store.get::<TaskDocument>(&key, TTL).await
            && !cached.stale
        {
            return Ok(cached.value);
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let details = self.client.get_task_details(game, homework_id).await?;
        let mut files = Vec::new();
        let mut size = 0usize;
        for path in &details.task.editable_paths {
            if files
                .iter()
                .any(|f: &crate::models::CodeFile| f.path == *path)
            {
                continue;
            }
            let file = self.client.get_file_content(game, path).await?;
            size += file.content.len();
            if size > 16 * 1024 * 1024 {
                return Err(Error::ResponseTooLarge);
            }
            files.push(file);
        }
        let document = TaskDocument { details, files };
        let _guard = self.writes.lock().await;
        if generation == self.generation.load(Ordering::SeqCst)
            && self
                .store
                .get::<bool>(&format!("pending:{game}"), Duration::MAX)
                .await
                .is_none()
        {
            self.store.put(&key, &document).await?;
        }
        Ok(document)
    }
}
