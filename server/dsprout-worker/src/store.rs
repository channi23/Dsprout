use anyhow::Result;
use dashmap::DashMap;
use std::{path::PathBuf, sync::Arc};
use tokio::fs;

#[derive(Debug, Clone, Copy)]
pub enum ShardSource {
    Ram,
    Disk,
}

#[derive(Debug, Clone)]
pub struct PreloadSummary {
    pub loaded: usize,
    pub missing: Vec<u8>,
}

#[derive(Clone)]
pub struct HotCache {
    // key: "file_id:seg:shard"
    pub map: Arc<DashMap<String, Vec<u8>>>,
}

impl HotCache {
    pub fn new() -> Self {
        Self {
            map: Arc::new(DashMap::new()),
        }
    }

    pub fn cache_key(file_id: &str, seg: u32, shard: u8) -> String {
        format!("{file_id}:{seg}:{shard}")
    }
}

fn base_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("No data_dir"))?;
    let dir = base.join("dsprout").join("worker_store");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn shard_path(file_id: &str, seg: u32, shard: u8) -> Result<PathBuf> {
    Ok(base_dir()?
        .join(file_id)
        .join(seg.to_string())
        .join(format!("{shard}.bin")))
}

pub async fn save_shard(file_id: &str, seg: u32, shard: u8, bytes: &[u8]) -> Result<()> {
    let dir = base_dir()?.join(file_id).join(seg.to_string());
    fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{shard}.bin"));
    fs::write(path, bytes).await?;
    Ok(())
}

pub async fn load_shard(file_id: &str, seg: u32, shard: u8) -> Result<Vec<u8>> {
    let path = shard_path(file_id, seg, shard)?;
    Ok(fs::read(path).await?)
}

pub async fn preload_shards(
    cache: &HotCache,
    file_id: &str,
    seg: u32,
    shard_indices: &[u8],
) -> Result<PreloadSummary> {
    let mut loaded = 0usize;
    let mut missing = Vec::new();

    for shard in shard_indices {
        let key = HotCache::cache_key(file_id, seg, *shard);
        if cache.map.contains_key(&key) {
            loaded += 1;
            continue;
        }

        match load_shard(file_id, seg, *shard).await {
            Ok(bytes) => {
                cache.map.insert(key, bytes);
                loaded += 1;
            }
            Err(_) => {
                missing.push(*shard);
            }
        }
    }

    Ok(PreloadSummary { loaded, missing })
}

pub async fn get_shard_prefer_cache(
    cache: &HotCache,
    file_id: &str,
    seg: u32,
    shard: u8,
) -> Result<(Vec<u8>, ShardSource)> {
    let key = HotCache::cache_key(file_id, seg, shard);

    if let Some(entry) = cache.map.get(&key) {
        return Ok((entry.clone(), ShardSource::Ram));
    }

    let bytes = load_shard(file_id, seg, shard).await?;
    Ok((bytes, ShardSource::Disk))
}
