use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use dashmap::DashMap;
use dsprout_common::models::{
    LocateResp, RegisterManifestReq, RegisterShardReq, ShardRecord, SignedManifest, WorkerInfo,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
struct AppState {
    // worker_id -> worker info
    workers: Arc<DashMap<String, WorkerInfo>>,
    // file_id -> list of shard records
    shard_index: Arc<DashMap<String, Vec<ShardRecord>>>,
    // file_id -> signed manifest
    manifest_index: Arc<DashMap<String, SignedManifest>>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegisterWorkerReq {
    worker_id: String,
    multiaddr: String,
}

#[derive(Debug, Clone, Deserialize)]
struct HeartbeatReq {
    worker_id: String,
    multiaddr: String,
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

async fn register_worker(
    state: axum::extract::State<AppState>,
    Json(req): Json<RegisterWorkerReq>,
) -> Json<&'static str> {
    let worker = WorkerInfo {
        worker_id: req.worker_id.clone(),
        multiaddr: req.multiaddr,
        last_seen: now_ms(),
    };
    state.workers.insert(req.worker_id, worker);
    Json("ok")
}

async fn workers(state: axum::extract::State<AppState>) -> Json<Vec<WorkerInfo>> {
    let out: Vec<WorkerInfo> = state.workers.iter().map(|e| e.value().clone()).collect();
    Json(out)
}

async fn heartbeat(
    state: axum::extract::State<AppState>,
    Json(req): Json<HeartbeatReq>,
) -> Json<&'static str> {
    let now = now_ms();
    state
        .workers
        .entry(req.worker_id.clone())
        .and_modify(|w| {
            w.last_seen = now;
            w.multiaddr = req.multiaddr.clone();
        })
        .or_insert(WorkerInfo {
            worker_id: req.worker_id,
            multiaddr: req.multiaddr,
            last_seen: now,
        });
    Json("ok")
}

async fn register_shard(
    state: axum::extract::State<AppState>,
    Json(req): Json<RegisterShardReq>,
) -> Json<&'static str> {
    state
        .shard_index
        .entry(req.record.file_id.clone())
        .and_modify(|v| v.push(req.record.clone()))
        .or_insert(vec![req.record]);
    Json("ok")
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

#[tokio::main]
async fn main() {
    let state = AppState {
        workers: Arc::new(DashMap::new()),
        shard_index: Arc::new(DashMap::new()),
        manifest_index: Arc::new(DashMap::new()),
    };

    let app = Router::new()
        .route("/register_worker", post(register_worker))
        .route("/workers", get(workers))
        .route("/heartbeat", post(heartbeat))
        .route("/register_shard", post(register_shard))
        .route("/locate", get(locate))
        .route("/register_manifest", post(register_manifest))
        .route("/manifest", get(get_manifest))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:7070").await.unwrap();
    println!("Satellite running on http://localhost:7070");
    axum::serve(listener, app).await.unwrap();
}
