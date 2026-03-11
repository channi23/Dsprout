use anyhow::Result;
use dashmap::DashMap;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tokio::fs as tfs;

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

#[derive(Debug, Clone)]
pub struct DiscoveredShard {
    pub file_id: String,
    pub segment_index: u32,
    pub shard_index: u8,
    pub bytes: Vec<u8>,
}

static STORE_PROFILE: OnceLock<String> = OnceLock::new();

pub fn set_store_profile(profile: String) {
    let _ = STORE_PROFILE.set(profile);
}

fn active_profile() -> String {
    STORE_PROFILE
        .get()
        .cloned()
        .unwrap_or_else(|| "worker".to_string())
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
    let dir = base
        .join("dsprout")
        .join("worker_store")
        .join(active_profile());
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn shard_path(file_id: &str, seg: u32, shard: u8) -> Result<PathBuf> {
    Ok(base_dir()?
        .join(file_id)
        .join(seg.to_string())
        .join(format!("{shard}.bin")))
}

pub fn scan_local_shards() -> Result<Vec<DiscoveredShard>> {
    let mut out = Vec::new();
    let base = base_dir()?;

    for file_entry in fs::read_dir(base)? {
        let file_entry = file_entry?;
        if !file_entry.file_type()?.is_dir() {
            continue;
        }

        let file_id = file_entry.file_name().to_string_lossy().to_string();
        let file_dir = file_entry.path();
        for seg_entry in fs::read_dir(file_dir)? {
            let seg_entry = seg_entry?;
            if !seg_entry.file_type()?.is_dir() {
                continue;
            }

            let seg_name = seg_entry.file_name().to_string_lossy().to_string();
            let Ok(segment_index) = seg_name.parse::<u32>() else {
                continue;
            };

            let seg_dir = seg_entry.path();
            for shard_entry in fs::read_dir(seg_dir)? {
                let shard_entry = shard_entry?;
                if !shard_entry.file_type()?.is_file() {
                    continue;
                }

                let name = shard_entry.file_name().to_string_lossy().to_string();
                let Some(stem) = name.strip_suffix(".bin") else {
                    continue;
                };
                let Ok(shard_index) = stem.parse::<u8>() else {
                    continue;
                };

                let bytes = fs::read(shard_entry.path())?;
                out.push(DiscoveredShard {
                    file_id: file_id.clone(),
                    segment_index,
                    shard_index,
                    bytes,
                });
            }
        }
    }

    Ok(out)
}

pub async fn save_shard(file_id: &str, seg: u32, shard: u8, bytes: &[u8]) -> Result<()> {
    let dir = base_dir()?.join(file_id).join(seg.to_string());
    tfs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{shard}.bin"));
    tfs::write(path, bytes).await?;
    Ok(())
}

pub async fn load_shard(file_id: &str, seg: u32, shard: u8) -> Result<Vec<u8>> {
    let path = shard_path(file_id, seg, shard)?;
    Ok(tfs::read(path).await?)
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
