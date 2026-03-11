use anyhow::{Result as AnyResult, anyhow};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use dsprout_common::identity;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{net::TcpListener, process::Child, process::Command, sync::Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct WorkerConfig {
    worker_id: String,
    profile: String,
    listen_multiaddr: String,
    advertise_multiaddr: String,
    satellite_url: String,
    device_name: String,
    owner_label: String,
    capacity_limit_bytes: u64,
    enabled: bool,
}

fn default_device_name() -> String {
    std::env::var("DSPROUT_DEVICE_NAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "worker-device".to_string())
}

fn worker_id_for_profile(profile: &str) -> AnyResult<String> {
    let kp = identity::load_or_create_keypair_for(profile)?;
    Ok(identity::peer_id_from_keypair(&kp).to_string())
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let profile = "worker".to_string();
        let worker_id = worker_id_for_profile(&profile).unwrap_or_else(|_| "".to_string());
        let listen_multiaddr = "/ip4/0.0.0.0/tcp/5901".to_string();
        Self {
            worker_id,
            profile,
            listen_multiaddr,
            advertise_multiaddr: "/ip4/127.0.0.1/tcp/5901".to_string(),
            satellite_url: "http://127.0.0.1:7070".to_string(),
            device_name: default_device_name(),
            owner_label: "Contributor".to_string(),
            capacity_limit_bytes: 1024 * 1024 * 1024,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SatelliteWorkerView {
    worker_id: String,
    multiaddr: String,
    device_name: String,
    owner_label: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
struct WorkerStatusResp {
    running: bool,
    pid: Option<u32>,
    started_at_ms: Option<u128>,
    last_exit_code: Option<i32>,
    last_error: Option<String>,
    config: WorkerConfig,
    satellite: Option<SatelliteWorkerView>,
    identity_match: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct ActionResp {
    status: String,
    message: String,
    worker: WorkerStatusResp,
}

#[derive(Debug, Clone, Serialize)]
struct StorageResp {
    profile: String,
    used_bytes: u64,
    hosted_shards: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigUpdateReq {
    profile: Option<String>,
    listen_multiaddr: Option<String>,
    advertise_multiaddr: Option<String>,
    satellite_url: Option<String>,
    device_name: Option<String>,
    owner_label: Option<String>,
    capacity_limit_bytes: Option<u64>,
    enabled: Option<bool>,
    restart_if_running: Option<bool>,
}

struct AgentState {
    config: WorkerConfig,
    child: Option<Child>,
    started_at_ms: Option<u128>,
    last_exit_code: Option<i32>,
    last_error: Option<String>,
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<AgentState>>,
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn to_http_err(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn data_dir() -> AnyResult<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow!("No data_dir found"))?;
    let dir = base.join("dsprout");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn config_path() -> AnyResult<PathBuf> {
    Ok(data_dir()?.join("agent-config.json"))
}

fn load_config() -> AnyResult<WorkerConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(WorkerConfig::default());
    }
    let bytes = fs::read(path)?;
    let mut cfg = serde_json::from_slice::<WorkerConfig>(&bytes)?;
    reconcile_config_identity(&mut cfg)?;
    Ok(cfg)
}

fn reconcile_config_identity(cfg: &mut WorkerConfig) -> AnyResult<()> {
    if cfg.profile.trim().is_empty() {
        cfg.profile = "worker".to_string();
    }
    if cfg.listen_multiaddr.trim().is_empty() {
        cfg.listen_multiaddr = "/ip4/0.0.0.0/tcp/5901".to_string();
    }
    if cfg.advertise_multiaddr.trim().is_empty() {
        cfg.advertise_multiaddr = cfg.listen_multiaddr.clone();
    }
    if cfg.device_name.trim().is_empty() {
        cfg.device_name = default_device_name();
    }
    if cfg.device_name == "Local Worker" || cfg.device_name == "Local_worker" {
        cfg.device_name = default_device_name();
    }
    if cfg.worker_id.trim().is_empty() {
        cfg.worker_id = worker_id_for_profile(&cfg.profile)?;
    } else {
        let derived = worker_id_for_profile(&cfg.profile)?;
        if cfg.worker_id != derived {
            cfg.worker_id = derived;
        }
    }
    Ok(())
}

fn save_config(cfg: &WorkerConfig) -> AnyResult<()> {
    let path = config_path()?;
    let bytes = serde_json::to_vec_pretty(cfg)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn worker_bin_path() -> AnyResult<PathBuf> {
    if let Some(v) = std::env::var_os("DSPROUT_WORKER_BIN") {
        let p = PathBuf::from(v);
        if p.exists() {
            return Ok(p);
        }
        return Err(anyhow!("DSPROUT_WORKER_BIN is set but path does not exist"));
    }

    let current = std::env::current_exe()?;
    let dir = current
        .parent()
        .ok_or_else(|| anyhow!("cannot resolve current exe parent"))?;
    let candidate = dir.join("dsprout-worker");
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(anyhow!(
        "failed to find dsprout-worker binary next to dsprout-agent; set DSPROUT_WORKER_BIN"
    ))
}

fn refresh_process_state(state: &mut AgentState) {
    if let Some(child) = state.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                state.last_exit_code = status.code();
                state.started_at_ms = None;
                state.child = None;
            }
            Ok(None) => {}
            Err(err) => {
                state.last_error = Some(format!("failed to poll worker process: {err}"));
                state.started_at_ms = None;
                state.child = None;
            }
        }
    }
}

fn snapshot_status(state: &AgentState) -> WorkerStatusResp {
    WorkerStatusResp {
        running: state.child.is_some(),
        pid: state.child.as_ref().and_then(Child::id),
        started_at_ms: state.started_at_ms,
        last_exit_code: state.last_exit_code,
        last_error: state.last_error.clone(),
        config: state.config.clone(),
        satellite: None,
        identity_match: None,
    }
}

async fn fetch_satellite_worker(
    satellite_url: &str,
    worker_id: &str,
) -> AnyResult<Option<SatelliteWorkerView>> {
    #[derive(Debug, Deserialize)]
    struct SatelliteWorkerInfo {
        worker_id: String,
        multiaddr: String,
        device_name: String,
        owner_label: String,
        enabled: bool,
    }

    let endpoint = format!(
        "{}/worker?worker_id={}",
        satellite_url.trim_end_matches('/'),
        urlencoding::encode(worker_id)
    );
    let client = reqwest::Client::new();
    let res = client.get(endpoint).send().await?;
    if res.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let res = res.error_for_status()?;
    let worker = res.json::<SatelliteWorkerInfo>().await?;
    Ok(Some(SatelliteWorkerView {
        worker_id: worker.worker_id,
        multiaddr: worker.multiaddr,
        device_name: worker.device_name,
        owner_label: worker.owner_label,
        enabled: worker.enabled,
    }))
}

async fn push_satellite_update(cfg: &WorkerConfig) -> AnyResult<()> {
    let endpoint = format!("{}/update_worker", cfg.satellite_url.trim_end_matches('/'));
    let payload = json!({
        "worker_id": cfg.worker_id,
        "multiaddr": cfg.advertise_multiaddr,
        "device_name": cfg.device_name,
        "owner_label": cfg.owner_label,
        "capacity_limit_bytes": cfg.capacity_limit_bytes,
        "enabled": cfg.enabled
    });
    let client = reqwest::Client::new();
    let res = client.post(endpoint).json(&payload).send().await?;
    if res.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    res.error_for_status()?;
    Ok(())
}

fn apply_config_update(
    cfg: &mut WorkerConfig,
    req: &ConfigUpdateReq,
) -> Result<(), (StatusCode, String)> {
    if let Some(v) = &req.profile {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "profile cannot be empty".to_string(),
            ));
        }
        cfg.profile = trimmed.to_string();
    }
    if let Some(v) = &req.listen_multiaddr {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "listen_multiaddr cannot be empty".to_string(),
            ));
        }
        cfg.listen_multiaddr = trimmed.to_string();
    }
    if let Some(v) = &req.advertise_multiaddr {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "advertise_multiaddr cannot be empty".to_string(),
            ));
        }
        cfg.advertise_multiaddr = trimmed.to_string();
    }
    if let Some(v) = &req.satellite_url {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "satellite_url cannot be empty".to_string(),
            ));
        }
        cfg.satellite_url = trimmed.to_string();
    }
    if let Some(v) = &req.device_name {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "device_name cannot be empty".to_string(),
            ));
        }
        cfg.device_name = trimmed.to_string();
    }
    if let Some(v) = &req.owner_label {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "owner_label cannot be empty".to_string(),
            ));
        }
        cfg.owner_label = trimmed.to_string();
    }
    if let Some(v) = req.capacity_limit_bytes {
        cfg.capacity_limit_bytes = v;
    }
    if let Some(v) = req.enabled {
        cfg.enabled = v;
    }
    if let Err(err) = reconcile_config_identity(cfg) {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
    }
    Ok(())
}

fn spawn_worker(cfg: &WorkerConfig) -> AnyResult<Child> {
    let bin = worker_bin_path()?;
    let mut cmd = Command::new(bin);
    cmd.arg("--profile")
        .arg(&cfg.profile)
        .arg("--listen")
        .arg(&cfg.listen_multiaddr)
        .arg("--advertise-multiaddr")
        .arg(&cfg.advertise_multiaddr)
        .arg("--satellite-url")
        .arg(&cfg.satellite_url)
        .arg("--device-name")
        .arg(&cfg.device_name)
        .arg("--owner-label")
        .arg(&cfg.owner_label)
        .arg("--capacity-limit-bytes")
        .arg(cfg.capacity_limit_bytes.to_string())
        .arg("--enabled")
        .arg(if cfg.enabled { "true" } else { "false" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    Ok(cmd.spawn()?)
}

fn worker_store_base(profile: &str) -> AnyResult<PathBuf> {
    Ok(data_dir()?.join("worker_store").join(profile))
}

fn scan_storage(profile: &str) -> AnyResult<StorageResp> {
    let base = worker_store_base(profile)?;
    if !base.exists() {
        return Ok(StorageResp {
            profile: profile.to_string(),
            used_bytes: 0,
            hosted_shards: 0,
        });
    }

    let mut used_bytes = 0u64;
    let mut hosted_shards = 0usize;

    fn walk(path: &Path, used: &mut u64, shards: &mut usize) -> AnyResult<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            let ftype = entry.file_type()?;
            if ftype.is_dir() {
                walk(&p, used, shards)?;
                continue;
            }
            if ftype.is_file() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.ends_with(".bin") {
                    *shards += 1;
                    *used += entry.metadata()?.len();
                }
            }
        }
        Ok(())
    }

    walk(&base, &mut used_bytes, &mut hosted_shards)?;

    Ok(StorageResp {
        profile: profile.to_string(),
        used_bytes,
        hosted_shards,
    })
}

async fn status(State(state): State<AppState>) -> Json<WorkerStatusResp> {
    let (mut out, cfg) = {
        let mut guard = state.inner.lock().await;
        refresh_process_state(&mut guard);
        (snapshot_status(&guard), guard.config.clone())
    };

    match fetch_satellite_worker(&cfg.satellite_url, &cfg.worker_id).await {
        Ok(satellite) => {
            out.identity_match = satellite.as_ref().map(|w| w.worker_id == cfg.worker_id);
            out.satellite = satellite;
        }
        Err(err) => {
            let prev = out.last_error.unwrap_or_default();
            out.last_error = Some(if prev.is_empty() {
                format!("satellite status lookup failed: {err}")
            } else {
                format!("{prev}; satellite status lookup failed: {err}")
            });
        }
    }

    Json(out)
}

async fn start(State(state): State<AppState>) -> Result<Json<ActionResp>, (StatusCode, String)> {
    let cfg = {
        let mut guard = state.inner.lock().await;
        refresh_process_state(&mut guard);
        if guard.child.is_some() {
            return Ok(Json(ActionResp {
                status: "ok".to_string(),
                message: "worker already running".to_string(),
                worker: snapshot_status(&guard),
            }));
        }
        guard.config.clone()
    };

    let child = spawn_worker(&cfg).map_err(to_http_err)?;

    let mut guard = state.inner.lock().await;
    guard.last_error = None;
    guard.last_exit_code = None;
    guard.started_at_ms = Some(now_ms());
    guard.child = Some(child);

    Ok(Json(ActionResp {
        status: "ok".to_string(),
        message: "worker started".to_string(),
        worker: snapshot_status(&guard),
    }))
}

async fn stop(State(state): State<AppState>) -> Result<Json<ActionResp>, (StatusCode, String)> {
    let mut child_opt = {
        let mut guard = state.inner.lock().await;
        refresh_process_state(&mut guard);
        guard.child.take()
    };

    if child_opt.is_none() {
        let mut guard = state.inner.lock().await;
        refresh_process_state(&mut guard);
        return Ok(Json(ActionResp {
            status: "ok".to_string(),
            message: "worker is not running".to_string(),
            worker: snapshot_status(&guard),
        }));
    }

    if let Some(child) = child_opt.as_mut() {
        let _ = child.kill().await;
        let status = child.wait().await.map_err(to_http_err)?;
        let mut guard = state.inner.lock().await;
        guard.started_at_ms = None;
        guard.last_exit_code = status.code();
    }

    let guard = state.inner.lock().await;
    Ok(Json(ActionResp {
        status: "ok".to_string(),
        message: "worker stopped".to_string(),
        worker: snapshot_status(&guard),
    }))
}

async fn config(
    State(state): State<AppState>,
    Json(req): Json<ConfigUpdateReq>,
) -> Result<Json<ActionResp>, (StatusCode, String)> {
    let (cfg, restart_if_running, mut old_child) = {
        let mut guard = state.inner.lock().await;
        refresh_process_state(&mut guard);

        apply_config_update(&mut guard.config, &req)?;
        save_config(&guard.config).map_err(to_http_err)?;

        let restart = req.restart_if_running.unwrap_or(true) && guard.child.is_some();
        let old_child = if restart { guard.child.take() } else { None };
        (guard.config.clone(), restart, old_child)
    };

    if let Some(child) = old_child.as_mut() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    if restart_if_running {
        let new_child = match spawn_worker(&cfg) {
            Ok(child) => child,
            Err(err) => {
                let mut guard = state.inner.lock().await;
                guard.started_at_ms = None;
                guard.last_error = Some(format!(
                    "failed to restart worker with updated config: {err}"
                ));
                return Err(to_http_err(err));
            }
        };

        let mut guard = state.inner.lock().await;
        guard.last_error = None;
        guard.last_exit_code = None;
        guard.started_at_ms = Some(now_ms());
        guard.child = Some(new_child);
    }

    let mut message = if restart_if_running {
        "config updated and worker restarted".to_string()
    } else {
        "config updated".to_string()
    };
    if let Err(err) = push_satellite_update(&cfg).await {
        message.push_str(&format!(" (satellite metadata sync pending: {err})"));
    }

    let mut guard = state.inner.lock().await;
    refresh_process_state(&mut guard);
    Ok(Json(ActionResp {
        status: "ok".to_string(),
        message,
        worker: snapshot_status(&guard),
    }))
}

async fn storage(State(state): State<AppState>) -> Result<Json<StorageResp>, (StatusCode, String)> {
    let profile = {
        let guard = state.inner.lock().await;
        guard.config.profile.clone()
    };
    let summary = scan_storage(&profile).map_err(to_http_err)?;
    Ok(Json(summary))
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    let cfg = load_config().unwrap_or_default();
    let _ = save_config(&cfg);
    let state = AppState {
        inner: Arc::new(Mutex::new(AgentState {
            config: cfg,
            child: None,
            started_at_ms: None,
            last_exit_code: None,
            last_error: None,
        })),
    };

    let bind_addr =
        std::env::var("DSPROUT_AGENT_BIND").unwrap_or_else(|_| "127.0.0.1:7081".to_string());
    let app = Router::new()
        .route("/status", get(status))
        .route("/start", post(start))
        .route("/stop", post(stop))
        .route("/config", post(config))
        .route("/storage", get(storage))
        .with_state(state);

    let listener = TcpListener::bind(&bind_addr).await?;
    println!("dsprout-agent listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
