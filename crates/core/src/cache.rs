//! Session-isolated persistent cache. Never stores credentials.
use crate::{Error, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

pub fn data_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("net", "Educoder", "EducoderPlusPlus")
        .map(|dirs| dirs.data_local_dir().to_owned())
        .ok_or(Error::InvalidInput("无法定位应用数据目录"))
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone)]
pub struct CacheStore {
    root: PathBuf,
    lock: Arc<Mutex<()>>,
}

#[derive(Serialize, Deserialize)]
struct Stored<T> {
    schema: u32,
    saved_at: u64,
    data: T,
}

pub struct Cached<T> {
    pub value: T,
    pub stale: bool,
}

impl CacheStore {
    pub fn new(namespace: &str) -> Result<Self> {
        Self::at(data_dir()?.join("cache"), namespace)
    }

    pub fn at(root: impl AsRef<Path>, namespace: &str) -> Result<Self> {
        if namespace.is_empty() {
            return Err(Error::InvalidInput("缓存命名空间不能为空"));
        }
        Ok(Self {
            root: root.as_ref().join(digest(namespace.as_bytes())),
            lock: Arc::new(Mutex::new(())),
        })
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{}.json", digest(key.as_bytes())))
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str, ttl: Duration) -> Option<Cached<T>> {
        let _guard = self.lock.lock().await;
        let path = self.path(key);
        if tokio::fs::metadata(&path).await.ok()?.len() > 64 * 1024 * 1024 {
            return None;
        }
        let bytes = tokio::fs::read(path).await.ok()?;
        let entry: Stored<T> = serde_json::from_slice(&bytes).ok()?;
        if entry.schema != 1 {
            return None;
        }
        let age = timestamp().checked_sub(entry.saved_at);
        Some(Cached {
            value: entry.data,
            stale: age.is_none_or(|age| age >= ttl.as_secs()),
        })
    }

    pub async fn put<T: Serialize + Sync>(&self, key: &str, data: &T) -> Result<()> {
        let bytes = serde_json::to_vec(&Stored {
            schema: 1,
            saved_at: timestamp(),
            data,
        })
        .map_err(|_| Error::Response("缓存序列化失败"))?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(Error::ResponseTooLarge);
        }
        let _guard = self.lock.lock().await;
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(Error::Io)?;
        let temp = self.root.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        tokio::fs::write(&temp, bytes).await.map_err(Error::Io)?;
        if let Err(error) = tokio::fs::rename(&temp, self.path(key)).await {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(Error::Io(error));
        }
        Ok(())
    }

    pub async fn remove(&self, key: &str) -> Result<()> {
        let _guard = self.lock.lock().await;
        match tokio::fs::remove_file(self.path(key)).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Io(error)),
        }
    }
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_is_persistent_isolated_and_corruption_is_a_miss() {
        let root = tempfile::tempdir().unwrap();
        let a = CacheStore::at(root.path(), "a").unwrap();
        let b = CacheStore::at(root.path(), "b").unwrap();
        a.put("courses", &vec![1, 2]).await.unwrap();
        let reloaded = CacheStore::at(root.path(), "a").unwrap();
        assert_eq!(
            reloaded
                .get::<Vec<i32>>("courses", Duration::from_secs(600))
                .await
                .unwrap()
                .value,
            vec![1, 2]
        );
        assert!(
            b.get::<Vec<i32>>("courses", Duration::from_secs(600))
                .await
                .is_none()
        );
        assert!(
            a.get::<Vec<i32>>("courses", Duration::ZERO)
                .await
                .unwrap()
                .stale
        );
        a.put("courses", &vec![3]).await.unwrap();
        assert_eq!(
            a.get::<Vec<i32>>("courses", Duration::from_secs(600))
                .await
                .unwrap()
                .value,
            vec![3]
        );
        tokio::fs::write(a.path("courses"), "broken").await.unwrap();
        assert!(
            a.get::<Vec<i32>>("courses", Duration::from_secs(600))
                .await
                .is_none()
        );
    }
}
