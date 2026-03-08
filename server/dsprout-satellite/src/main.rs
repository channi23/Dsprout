use anyhow::{Result as AnyResult, anyhow};
use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use dashmap::DashMap;
use dsprout_common::models::{
    LocateResp, RegisterManifestReq, RegisterShardReq, RegisterWorkerReq, ShardRecord,
    SignedManifest, UpdateWorkerReq, WorkerInfo,
};
use rusqlite::{Connection, params};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct PersistentStore {
    conn: Arc<Mutex<Connection>>,
}

impl PersistentStore {
    fn open_default() -> AnyResult<Self> {
        let base = dirs::data_dir().ok_or_else(|| anyhow!("No data_dir found"))?;
        let dir = base.join("dsprout");
        fs::create_dir_all(&dir)?;
        Self::open(&dir.join("satellite.sqlite3"))
    }

    fn open(path: &Path) -> AnyResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> AnyResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS workers (
                worker_id TEXT PRIMARY KEY,
                multiaddr TEXT NOT NULL,
                device_name TEXT NOT NULL DEFAULT '',
                owner_label TEXT NOT NULL DEFAULT '',
                capacity_limit_bytes INTEGER NOT NULL DEFAULT 0,
                used_bytes INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_seen INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS shard_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id TEXT NOT NULL,
                segment_index INTEGER NOT NULL,
                shard_index INTEGER NOT NULL,
                worker_id TEXT NOT NULL,
                worker_multiaddr TEXT NOT NULL,
                shard_hash_hex TEXT NOT NULL,
                UNIQUE(file_id, segment_index, shard_index, worker_id)
            );

            CREATE INDEX IF NOT EXISTS idx_shards_file_id ON shard_records(file_id);

            DELETE FROM shard_records
            WHERE rowid NOT IN (
                SELECT MIN(rowid)
                FROM shard_records
                GROUP BY file_id, segment_index, shard_index, worker_id
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_shards_unique
              ON shard_records(file_id, segment_index, shard_index, worker_id);

            CREATE TABLE IF NOT EXISTS manifests (
                file_id TEXT PRIMARY KEY,
                signed_manifest_json TEXT NOT NULL
            );
            ",
        )?;

        let existing_cols: HashSet<String> = {
            let mut cols = HashSet::new();
            let mut stmt = conn.prepare("PRAGMA table_info(workers)")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let col_name: String = row.get(1)?;
                cols.insert(col_name);
            }
            cols
        };

        if !existing_cols.contains("device_name") {
            conn.execute(
                "ALTER TABLE workers ADD COLUMN device_name TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !existing_cols.contains("owner_label") {
            conn.execute(
                "ALTER TABLE workers ADD COLUMN owner_label TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !existing_cols.contains("capacity_limit_bytes") {
            conn.execute(
                "ALTER TABLE workers ADD COLUMN capacity_limit_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !existing_cols.contains("used_bytes") {
            conn.execute(
                "ALTER TABLE workers ADD COLUMN used_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !existing_cols.contains("enabled") {
            conn.execute(
                "ALTER TABLE workers ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }

        Ok(())
    }

    fn load_workers(&self) -> AnyResult<Vec<WorkerInfo>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        let mut stmt = conn.prepare(
            "
            SELECT worker_id, multiaddr, device_name, owner_label,
                   capacity_limit_bytes, used_bytes, enabled, last_seen
            FROM workers
            ",
        )?;
        let mut rows = stmt.query([])?;

        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let capacity_limit_bytes: i64 = row.get(4)?;
            let used_bytes: i64 = row.get(5)?;
            let enabled: i64 = row.get(6)?;
            let last_seen: i64 = row.get(7)?;
            out.push(WorkerInfo {
                worker_id: row.get(0)?,
                multiaddr: row.get(1)?,
                device_name: row.get(2)?,
                owner_label: row.get(3)?,
                capacity_limit_bytes: capacity_limit_bytes as u64,
                used_bytes: used_bytes as u64,
                enabled: enabled != 0,
                last_seen: last_seen as u128,
            });
        }
        Ok(out)
    }

    fn load_shards(&self) -> AnyResult<Vec<ShardRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        let mut stmt = conn.prepare(
            "
            SELECT worker_id, worker_multiaddr, file_id, segment_index, shard_index, shard_hash_hex
            FROM shard_records
            ",
        )?;
        let mut rows = stmt.query([])?;

        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let segment_index: i64 = row.get(3)?;
            let shard_index: i64 = row.get(4)?;
            out.push(ShardRecord {
                worker_id: row.get(0)?,
                worker_multiaddr: row.get(1)?,
                file_id: row.get(2)?,
                segment_index: segment_index as u32,
                shard_index: shard_index as u8,
                shard_hash_hex: row.get(5)?,
            });
        }
        Ok(out)
    }

    fn load_manifests(&self) -> AnyResult<Vec<SignedManifest>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        let mut stmt = conn.prepare("SELECT signed_manifest_json FROM manifests")?;
        let mut rows = stmt.query([])?;

        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let manifest: SignedManifest = serde_json::from_str(&json)?;
            out.push(manifest);
        }
        Ok(out)
    }

    fn upsert_worker(&self, worker: &WorkerInfo) -> AnyResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        conn.execute(
            "
            INSERT INTO workers(
              worker_id, multiaddr, device_name, owner_label,
              capacity_limit_bytes, used_bytes, enabled, last_seen
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(worker_id) DO UPDATE SET
              multiaddr = excluded.multiaddr,
              device_name = excluded.device_name,
              owner_label = excluded.owner_label,
              capacity_limit_bytes = excluded.capacity_limit_bytes,
              used_bytes = excluded.used_bytes,
              enabled = excluded.enabled,
              last_seen = excluded.last_seen
            ",
            params![
                worker.worker_id,
                worker.multiaddr,
                worker.device_name,
                worker.owner_label,
                worker.capacity_limit_bytes as i64,
                worker.used_bytes as i64,
                if worker.enabled { 1 } else { 0 },
                worker.last_seen as i64,
            ],
        )?;
        Ok(())
    }

    fn insert_shard(&self, rec: &ShardRecord) -> AnyResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        conn.execute(
            "
            INSERT INTO shard_records(
              file_id, segment_index, shard_index, worker_id, worker_multiaddr, shard_hash_hex
            ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(file_id, segment_index, shard_index, worker_id) DO UPDATE SET
              worker_multiaddr = excluded.worker_multiaddr,
              shard_hash_hex = excluded.shard_hash_hex
            ",
            params![
                rec.file_id,
                rec.segment_index as i64,
                rec.shard_index as i64,
                rec.worker_id,
                rec.worker_multiaddr,
                rec.shard_hash_hex,
            ],
        )?;
        Ok(())
    }

    fn upsert_manifest(&self, signed_manifest: &SignedManifest) -> AnyResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        let file_id = signed_manifest.manifest.file_id.clone();
        let json = serde_json::to_string(signed_manifest)?;
        conn.execute(
            "
            INSERT INTO manifests(file_id, signed_manifest_json)
            VALUES(?1, ?2)
            ON CONFLICT(file_id) DO UPDATE SET
              signed_manifest_json = excluded.signed_manifest_json
            ",
            params![file_id, json],
        )?;
        Ok(())
    }
}

#[derive(Clone)]
struct AppState {
    workers: Arc<DashMap<String, WorkerInfo>>,
    shard_index: Arc<DashMap<String, Vec<ShardRecord>>>,
    manifest_index: Arc<DashMap<String, SignedManifest>>,
    store: PersistentStore,
}

#[derive(Debug, Clone, Deserialize)]
struct HeartbeatReq {
    worker_id: String,
    multiaddr: String,
    device_name: Option<String>,
    owner_label: Option<String>,
    capacity_limit_bytes: Option<u64>,
    used_bytes: Option<u64>,
    enabled: Option<bool>,
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

fn to_internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn upsert_shard_in_memory(state: &AppState, rec: ShardRecord) {
    state
        .shard_index
        .entry(rec.file_id.clone())
        .and_modify(|v| {
            if let Some(existing) = v.iter_mut().find(|existing| {
                existing.file_id == rec.file_id
                    && existing.segment_index == rec.segment_index
                    && existing.shard_index == rec.shard_index
                    && existing.worker_id == rec.worker_id
            }) {
                *existing = rec.clone();
            } else {
                v.push(rec.clone());
            }
        })
        .or_insert(vec![rec]);
}

async fn register_worker(
    state: axum::extract::State<AppState>,
    Json(req): Json<RegisterWorkerReq>,
) -> Result<Json<&'static str>, (StatusCode, String)> {
    let worker = WorkerInfo {
        worker_id: req.worker_id.clone(),
        multiaddr: req.multiaddr,
        device_name: req.device_name,
        owner_label: req.owner_label,
        capacity_limit_bytes: req.capacity_limit_bytes,
        used_bytes: req.used_bytes,
        enabled: req.enabled,
        last_seen: now_ms(),
    };

    state.store.upsert_worker(&worker).map_err(to_internal_error)?;
    state.workers.insert(req.worker_id, worker);
    Ok(Json("ok"))
}

async fn update_worker(
    state: axum::extract::State<AppState>,
    Json(req): Json<UpdateWorkerReq>,
) -> Result<Json<WorkerInfo>, (StatusCode, String)> {
    let existing = state
        .workers
        .get(&req.worker_id)
        .map(|v| v.clone())
        .ok_or_else(|| (StatusCode::NOT_FOUND, "worker not found".to_string()))?;

    let updated = WorkerInfo {
        worker_id: existing.worker_id.clone(),
        multiaddr: req.multiaddr.unwrap_or(existing.multiaddr),
        device_name: req.device_name.unwrap_or(existing.device_name),
        owner_label: req.owner_label.unwrap_or(existing.owner_label),
        capacity_limit_bytes: req
            .capacity_limit_bytes
            .unwrap_or(existing.capacity_limit_bytes),
        used_bytes: req.used_bytes.unwrap_or(existing.used_bytes),
        enabled: req.enabled.unwrap_or(existing.enabled),
        last_seen: now_ms(),
    };

    state.store.upsert_worker(&updated).map_err(to_internal_error)?;
    state
        .workers
        .insert(updated.worker_id.clone(), updated.clone());
    Ok(Json(updated))
}

async fn workers(state: axum::extract::State<AppState>) -> Json<Vec<WorkerInfo>> {
    let out: Vec<WorkerInfo> = state.workers.iter().map(|e| e.value().clone()).collect();
    Json(out)
}

async fn get_worker(
    state: axum::extract::State<AppState>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<WorkerInfo>, (StatusCode, String)> {
    let worker_id = q.get("worker_id").cloned().unwrap_or_default();
    if worker_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "worker_id is required".to_string()));
    }

    match state.workers.get(&worker_id) {
        Some(v) => Ok(Json(v.clone())),
        None => Err((StatusCode::NOT_FOUND, "worker not found".to_string())),
    }
}

async fn heartbeat(
    state: axum::extract::State<AppState>,
    Json(req): Json<HeartbeatReq>,
) -> Result<Json<&'static str>, (StatusCode, String)> {
    let now = now_ms();
    let existing = state.workers.get(&req.worker_id).map(|v| v.clone());

    let worker = WorkerInfo {
        worker_id: req.worker_id.clone(),
        multiaddr: req.multiaddr.clone(),
        device_name: req
            .device_name
            .or_else(|| existing.as_ref().map(|w| w.device_name.clone()))
            .unwrap_or_else(|| "unknown-device".to_string()),
        owner_label: req
            .owner_label
            .or_else(|| existing.as_ref().map(|w| w.owner_label.clone()))
            .unwrap_or_else(|| "unknown-owner".to_string()),
        capacity_limit_bytes: req
            .capacity_limit_bytes
            .or_else(|| existing.as_ref().map(|w| w.capacity_limit_bytes))
            .unwrap_or(0),
        used_bytes: req
            .used_bytes
            .or_else(|| existing.as_ref().map(|w| w.used_bytes))
            .unwrap_or(0),
        enabled: req
            .enabled
            .or_else(|| existing.as_ref().map(|w| w.enabled))
            .unwrap_or(true),
        last_seen: now,
    };

    state.store.upsert_worker(&worker).map_err(to_internal_error)?;
    state.workers.insert(req.worker_id, worker);
    Ok(Json("ok"))
}

async fn register_shard(
    state: axum::extract::State<AppState>,
    Json(req): Json<RegisterShardReq>,
) -> Result<Json<&'static str>, (StatusCode, String)> {
    state
        .store
        .insert_shard(&req.record)
        .map_err(to_internal_error)?;
    upsert_shard_in_memory(&state, req.record);
    Ok(Json("ok"))
}

async fn locate(
    state: axum::extract::State<AppState>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Json<LocateResp> {
    let file_id = q.get("file_id").cloned().unwrap_or_default();
    let shards = state
        .shard_index
        .get(&file_id)
        .map(|r| r.clone())
        .unwrap_or_default();

    Json(LocateResp { file_id, shards })
}

async fn register_manifest(
    state: axum::extract::State<AppState>,
    Json(req): Json<RegisterManifestReq>,
) -> Result<Json<&'static str>, (StatusCode, String)> {
    req.signed_manifest
        .verify()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid manifest signature: {e}")))?;

    state
        .store
        .upsert_manifest(&req.signed_manifest)
        .map_err(to_internal_error)?;

    let file_id = req.signed_manifest.manifest.file_id.clone();
    state.manifest_index.insert(file_id, req.signed_manifest);
    Ok(Json("ok"))
}

async fn get_manifest(
    state: axum::extract::State<AppState>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<SignedManifest>, (StatusCode, String)> {
    let file_id = q.get("file_id").cloned().unwrap_or_default();
    if file_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "file_id is required".to_string()));
    }

    match state.manifest_index.get(&file_id) {
        Some(v) => Ok(Json(v.clone())),
        None => Err((StatusCode::NOT_FOUND, "manifest not found".to_string())),
    }
}

fn load_state_from_store(store: &PersistentStore) -> AnyResult<AppState> {
    let workers_loaded = store.load_workers()?;
    let shards_loaded = store.load_shards()?;
    let manifests_loaded = store.load_manifests()?;

    let workers = Arc::new(DashMap::new());
    for w in workers_loaded {
        workers.insert(w.worker_id.clone(), w);
    }

    let shard_index: Arc<DashMap<String, Vec<ShardRecord>>> = Arc::new(DashMap::new());
    for rec in shards_loaded {
        shard_index
            .entry(rec.file_id.clone())
            .and_modify(|v| {
                if let Some(existing) = v.iter_mut().find(|existing| {
                    existing.file_id == rec.file_id
                        && existing.segment_index == rec.segment_index
                        && existing.shard_index == rec.shard_index
                        && existing.worker_id == rec.worker_id
                }) {
                    *existing = rec.clone();
                } else {
                    v.push(rec.clone());
                }
            })
            .or_insert(vec![rec]);
    }

    let manifest_index = Arc::new(DashMap::new());
    for m in manifests_loaded {
        manifest_index.insert(m.manifest.file_id.clone(), m);
    }

    Ok(AppState {
        workers,
        shard_index,
        manifest_index,
        store: store.clone(),
    })
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    let store = PersistentStore::open_default()?;
    let state = load_state_from_store(&store)?;

    println!(
        "Loaded persisted state: workers={} files_with_shards={} manifests={} db={}",
        state.workers.len(),
        state.shard_index.len(),
        state.manifest_index.len(),
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("dsprout")
            .join("satellite.sqlite3")
            .display()
    );

    let app = Router::new()
        .route("/register_worker", post(register_worker))
        .route("/update_worker", post(update_worker))
        .route("/worker", get(get_worker))
        .route("/workers", get(workers))
        .route("/heartbeat", post(heartbeat))
        .route("/register_shard", post(register_shard))
        .route("/locate", get(locate))
        .route("/register_manifest", post(register_manifest))
        .route("/manifest", get(get_manifest))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:7070").await?;
    println!("Satellite running on http://localhost:7070");
    axum::serve(listener, app).await?;
    Ok(())
}
