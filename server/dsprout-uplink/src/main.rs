use anyhow::Result;
use dsprout_common::{
    crypto, hash,
    identity,
    models::{
        FileManifest, LocateResp, ManifestSegment, RS_K, RS_N, RegisterManifestReq,
        RegisterShardReq, ShardRecord, SignedManifest, WorkerInfo, SEGMENT_SIZE,
    },
    net::{
        DsproutBehaviour, DsproutEvent, build_swarm,
        hello::{NetRequest, NetResponse},
    },
    sharding,
};
use futures::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm, request_response, swarm::SwarmEvent};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    path::PathBuf,
    time::Duration,
};

const ROOT_SECRET: &[u8] = b"dsprout-dev-root-secret-v1";
const WORKER_HEALTH_MAX_AGE_MS: u128 = 30_000;

#[derive(Debug, Clone)]
struct CommonArgs {
    satellite_url: String,
    swarm_key: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct UploadArgs {
    common: CommonArgs,
    input: PathBuf,
    file_id: Option<String>,
    replication_factor: usize,
}

#[derive(Debug, Clone)]
struct DownloadArgs {
    common: CommonArgs,
    file_id: String,
    output: PathBuf,
}

#[derive(Debug, Clone)]
struct RepairArgs {
    common: CommonArgs,
    file_id: String,
    replication_factor: usize,
}

#[derive(Debug, Clone)]
enum Command {
    Upload(UploadArgs),
    Download(DownloadArgs),
    Repair(RepairArgs),
}

struct SatelliteClient {
    base_url: String,
    http: reqwest::Client,
}

impl SatelliteClient {
    fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    async fn register_shard(&self, record: ShardRecord) -> Result<()> {
        let req = RegisterShardReq { record };
        let res = self
            .http
            .post(self.endpoint("/register_shard"))
            .json(&req)
            .send()
            .await?;
        res.error_for_status()?;
        Ok(())
    }

    async fn locate(&self, file_id: &str) -> Result<LocateResp> {
        let res = self
            .http
            .get(self.endpoint("/locate"))
            .query(&[("file_id", file_id)])
            .send()
            .await?;
        let res = res.error_for_status()?;
        Ok(res.json::<LocateResp>().await?)
    }

    async fn workers(&self) -> Result<Vec<WorkerInfo>> {
        let res = self.http.get(self.endpoint("/workers")).send().await?;
        let res = res.error_for_status()?;
        Ok(res.json::<Vec<WorkerInfo>>().await?)
    }

    async fn register_manifest(&self, signed_manifest: &SignedManifest) -> Result<()> {
        let req = RegisterManifestReq {
            signed_manifest: signed_manifest.clone(),
        };
        let res = self
            .http
            .post(self.endpoint("/register_manifest"))
            .json(&req)
            .send()
            .await?;
        res.error_for_status()?;
        Ok(())
    }

    async fn manifest(&self, file_id: &str) -> Result<SignedManifest> {
        let res = self
            .http
            .get(self.endpoint("/manifest"))
            .query(&[("file_id", file_id)])
            .send()
            .await?;
        let res = res.error_for_status()?;
        Ok(res.json::<SignedManifest>().await?)
    }
}

struct ControlClient {
    swarm: Swarm<DsproutBehaviour>,
    target_peer: Option<PeerId>,
}

impl ControlClient {
    fn new(swarm: Swarm<DsproutBehaviour>) -> Self {
        Self {
            swarm,
            target_peer: None,
        }
    }

    fn target_peer(&self) -> Result<PeerId> {
        self.target_peer
            .ok_or_else(|| anyhow::anyhow!("not connected to worker"))
    }

    fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    async fn connect(&mut self, dial: Multiaddr) -> Result<PeerId> {
        self.swarm.dial(dial)?;

        let timeout = tokio::time::sleep(Duration::from_secs(15));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    return Err(anyhow::anyhow!("timed out waiting for worker connection"));
                }
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            self.target_peer = Some(peer_id);
                        }
                        SwarmEvent::Behaviour(DsproutEvent::Identify(libp2p::identify::Event::Received { peer_id, .. })) => {
                            if Some(peer_id) == self.target_peer {
                                return Ok(peer_id);
                            }
                        }
                        SwarmEvent::OutgoingConnectionError { error, .. } => {
                            return Err(anyhow::anyhow!("dial failed: {error}"));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn request(&mut self, request: NetRequest) -> Result<NetResponse> {
        let peer = self.target_peer()?;
        let request_id = self
            .swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer, request);

        let timeout = tokio::time::sleep(Duration::from_secs(30));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    return Err(anyhow::anyhow!("timed out waiting for worker response"));
                }
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(DsproutEvent::RequestResponse(request_response::Event::Message {
                            peer: response_peer,
                            message: request_response::Message::Response { request_id: response_id, response },
                        })) if response_peer == peer && response_id == request_id => {
                            return Ok(response);
                        }
                        SwarmEvent::Behaviour(DsproutEvent::RequestResponse(request_response::Event::OutboundFailure {
                            request_id: failed_id,
                            error,
                            ..
                        })) if failed_id == request_id => {
                            return Err(anyhow::anyhow!("request failed: {error}"));
                        }
                        SwarmEvent::OutgoingConnectionError { error, .. } => {
                            return Err(anyhow::anyhow!("connection error: {error}"));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

struct WorkerConnection {
    worker_id: String,
    multiaddr: Multiaddr,
    client: ControlClient,
    online: bool,
}

fn parse_command() -> Result<Command> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err(anyhow::anyhow!(
            "usage: dsprout-uplink <upload|download|repair> [args]"
        ));
    }

    let subcommand = args.remove(0);
    let mut satellite_url: String = "http://127.0.0.1:7070".to_string();
    let mut swarm_key: Option<PathBuf> = None;

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut file_id: Option<String> = None;
    let mut replication_factor: usize = 2;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--satellite-url" => {
                i += 1;
                satellite_url = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --satellite-url"))?
                    .clone();
            }
            "--swarm-key" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --swarm-key"))?;
                swarm_key = Some(PathBuf::from(v));
            }
            "--input" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --input"))?;
                input = Some(PathBuf::from(v));
            }
            "--output" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --output"))?;
                output = Some(PathBuf::from(v));
            }
            "--file-id" => {
                i += 1;
                file_id = Some(
                    args.get(i)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --file-id"))?
                        .clone(),
                );
            }
            "--replication-factor" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --replication-factor"))?;
                replication_factor = v.parse::<usize>().map_err(|e| {
                    anyhow::anyhow!("invalid value for --replication-factor ({v}): {e}")
                })?;
            }
            unknown => {
                return Err(anyhow::anyhow!("unknown argument: {unknown}"));
            }
        }
        i += 1;
    }

    let common = CommonArgs {
        satellite_url,
        swarm_key,
    };

    match subcommand.as_str() {
        "upload" => Ok(Command::Upload(UploadArgs {
            common,
            input: input.ok_or_else(|| anyhow::anyhow!("--input is required for upload"))?,
            file_id,
            replication_factor,
        })),
        "download" => Ok(Command::Download(DownloadArgs {
            common,
            file_id: file_id
                .ok_or_else(|| anyhow::anyhow!("--file-id is required for download"))?,
            output: output.ok_or_else(|| anyhow::anyhow!("--output is required for download"))?,
        })),
        "repair" => Ok(Command::Repair(RepairArgs {
            common,
            file_id: file_id
                .ok_or_else(|| anyhow::anyhow!("--file-id is required for repair"))?,
            replication_factor,
        })),
        _ => Err(anyhow::anyhow!("unknown subcommand: {subcommand}")),
    }
}

fn uplink_meta_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("No data_dir found"))?;
    let dir = base.join("dsprout").join("uplink_meta");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn manifest_path(file_id: &str) -> Result<PathBuf> {
    Ok(uplink_meta_dir()?.join(format!("{file_id}.json")))
}

fn save_manifest(manifest: &SignedManifest) -> Result<PathBuf> {
    let path = manifest_path(&manifest.manifest.file_id)?;
    let bytes = serde_json::to_vec_pretty(manifest)?;
    fs::write(&path, bytes)?;
    Ok(path)
}

fn load_manifest(file_id: &str) -> Result<SignedManifest> {
    let bytes = fs::read(manifest_path(file_id)?)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn create_file_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

async fn connect_worker(common: &CommonArgs, multiaddr: Multiaddr) -> Result<WorkerConnection> {
    let swarm_key = common
        .swarm_key
        .clone()
        .unwrap_or_else(|| dsprout_common::net::default_swarm_key_path(env!("CARGO_MANIFEST_DIR")));
    let mut client = ControlClient::new(build_swarm(&swarm_key, "uplink")?);
    let worker_id = client.connect(multiaddr.clone()).await?.to_string();

    match client
        .request(NetRequest::Hello {
            from_peer: client.local_peer_id().to_string(),
            message: "hello-upload-download".to_string(),
        })
        .await?
    {
        NetResponse::HelloAck { .. } => {}
        other => {
            return Err(anyhow::anyhow!("unexpected hello response: {other:?}"));
        }
    }

    Ok(WorkerConnection {
        worker_id,
        multiaddr,
        client,
        online: true,
    })
}

async fn resolve_upload_workers(
    common: &CommonArgs,
    satellite: &SatelliteClient,
) -> Result<Vec<WorkerConnection>> {
    let healthy_workers = discover_healthy_workers(satellite).await?;
    let worker_addrs: Vec<Multiaddr> = healthy_workers
        .iter()
        .filter_map(|w| w.multiaddr.parse::<Multiaddr>().ok())
        .collect();

    if worker_addrs.is_empty() {
        return Err(anyhow::anyhow!("no healthy workers discovered from /workers"));
    }

    let mut out = Vec::new();
    for addr in worker_addrs {
        match connect_worker(common, addr.clone()).await {
            Ok(conn) => out.push(conn),
            Err(err) => {
                eprintln!("warning: failed to connect worker {addr}: {err}");
            }
        }
    }

    if out.is_empty() {
        return Err(anyhow::anyhow!("failed to connect to any worker"));
    }

    Ok(out)
}

async fn discover_healthy_workers(satellite: &SatelliteClient) -> Result<Vec<WorkerInfo>> {
    let now = now_ms();
    let mut healthy_workers = Vec::new();
    let mut unhealthy = 0usize;
    let mut invalid_addr = 0usize;
    for w in satellite.workers().await? {
        if now.saturating_sub(w.last_seen) > WORKER_HEALTH_MAX_AGE_MS {
            unhealthy += 1;
            continue;
        }
        match w.multiaddr.parse::<Multiaddr>() {
            Ok(_) => healthy_workers.push(w),
            Err(_) => {
                invalid_addr += 1;
            }
        }
    }

    if healthy_workers.is_empty() {
        return Err(anyhow::anyhow!("no healthy workers discovered from /workers"));
    }

    if unhealthy > 0 || invalid_addr > 0 {
        eprintln!(
            "worker discovery filtered: unhealthy={} invalid_multiaddr={}",
            unhealthy, invalid_addr
        );
    }

    Ok(healthy_workers)
}

async fn run_repair(args: RepairArgs) -> Result<()> {
    if args.replication_factor == 0 {
        return Err(anyhow::anyhow!(
            "--replication-factor must be >= 1 for repair"
        ));
    }

    let satellite = SatelliteClient::new(args.common.satellite_url.clone());
    let locate = satellite.locate(&args.file_id).await?;
    if locate.shards.is_empty() {
        return Err(anyhow::anyhow!("satellite returned no shards for file"));
    }

    let healthy_workers = discover_healthy_workers(&satellite).await?;
    let mut workers: HashMap<String, WorkerConnection> = HashMap::new();
    for w in healthy_workers {
        let Ok(addr) = w.multiaddr.parse::<Multiaddr>() else {
            continue;
        };
        match connect_worker(&args.common, addr.clone()).await {
            Ok(conn) => {
                workers.insert(w.worker_id, conn);
            }
            Err(err) => {
                eprintln!(
                    "warning: healthy worker {} unreachable for repair ({addr}): {err}",
                    w.worker_id
                );
            }
        }
    }

    if workers.is_empty() {
        return Err(anyhow::anyhow!("no healthy workers reachable for repair"));
    }

    let mut shard_groups: BTreeMap<(u32, u8), Vec<ShardRecord>> = BTreeMap::new();
    for rec in locate.shards {
        shard_groups
            .entry((rec.segment_index, rec.shard_index))
            .or_default()
            .push(rec);
    }

    let mut total_shards = 0usize;
    let mut repaired_shards = 0usize;
    let mut new_replicas = 0usize;

    for ((segment_index, shard_index), records) in shard_groups {
        total_shards += 1;
        let mut existing_ids: HashSet<String> = records.iter().map(|r| r.worker_id.clone()).collect();

        let healthy_records: Vec<&ShardRecord> = records
            .iter()
            .filter(|r| workers.get(&r.worker_id).is_some_and(|w| w.online))
            .collect();
        let healthy_count = healthy_records
            .iter()
            .map(|r| r.worker_id.as_str())
            .collect::<HashSet<_>>()
            .len();

        if healthy_count >= args.replication_factor {
            continue;
        }

        let source = healthy_records.first().copied();
        let Some(source) = source else {
            eprintln!(
                "warning: no healthy source for segment={} shard={}",
                segment_index, shard_index
            );
            continue;
        };

        let bytes = {
            let Some(source_conn) = workers.get_mut(&source.worker_id) else {
                continue;
            };
            match source_conn
                .client
                .request(NetRequest::VerifyGet {
                    file_id: args.file_id.clone(),
                    segment_index,
                    shard_index,
                })
                .await
            {
                Ok(NetResponse::VerifyGetOk { bytes, .. }) => bytes,
                Ok(other) => {
                    eprintln!(
                        "warning: source verify_get unexpected segment={} shard={} worker={} resp={other:?}",
                        segment_index, shard_index, source.worker_id
                    );
                    continue;
                }
                Err(err) => {
                    eprintln!(
                        "warning: source verify_get failed segment={} shard={} worker={}: {}",
                        segment_index, shard_index, source.worker_id, err
                    );
                    continue;
                }
            }
        };

        let actual_hash = hash::blake3_hash_hex(&bytes);
        if actual_hash != source.shard_hash_hex {
            eprintln!(
                "warning: source hash mismatch for segment={} shard={} worker={}",
                segment_index, shard_index, source.worker_id
            );
            continue;
        }

        let mut repaired_this = false;
        let mut healthy_now = healthy_count;
        for (worker_id, worker_conn) in workers.iter_mut() {
            if healthy_now >= args.replication_factor {
                break;
            }
            if existing_ids.contains(worker_id) || !worker_conn.online {
                continue;
            }

            match worker_conn
                .client
                .request(NetRequest::StoreShard {
                    file_id: args.file_id.clone(),
                    segment_index,
                    shard_index,
                    bytes: bytes.clone(),
                })
                .await
            {
                Ok(NetResponse::StoreShardAck { stored: true, .. }) => {
                    satellite
                        .register_shard(ShardRecord {
                            worker_id: worker_conn.worker_id.clone(),
                            worker_multiaddr: worker_conn.multiaddr.to_string(),
                            file_id: args.file_id.clone(),
                            segment_index,
                            shard_index,
                            shard_hash_hex: actual_hash.clone(),
                        })
                        .await?;
                    existing_ids.insert(worker_id.clone());
                    healthy_now += 1;
                    new_replicas += 1;
                    repaired_this = true;
                }
                Ok(NetResponse::StoreShardAck { stored: false, .. }) => {
                    eprintln!(
                        "warning: repair store reported stored=false segment={} shard={} target={}",
                        segment_index, shard_index, worker_id
                    );
                }
                Ok(NetResponse::Error { message }) => {
                    eprintln!(
                        "warning: repair store error segment={} shard={} target={}: {}",
                        segment_index, shard_index, worker_id, message
                    );
                }
                Ok(other) => {
                    eprintln!(
                        "warning: repair store unexpected response segment={} shard={} target={}: {other:?}",
                        segment_index, shard_index, worker_id
                    );
                }
                Err(err) => {
                    eprintln!(
                        "warning: repair store failed segment={} shard={} target={}: {}",
                        segment_index, shard_index, worker_id, err
                    );
                    worker_conn.online = false;
                }
            }
        }

        if repaired_this {
            repaired_shards += 1;
        }
    }

    println!("Repair complete");
    println!("file_id={}", args.file_id);
    println!("target_replication_factor={}", args.replication_factor);
    println!("healthy_workers_reachable={}", workers.len());
    println!("total_shards={total_shards}");
    println!("repaired_shards={repaired_shards}");
    println!("new_replicas={new_replicas}");
    Ok(())
}

async fn run_upload(args: UploadArgs) -> Result<()> {
    let satellite = SatelliteClient::new(args.common.satellite_url.clone());
    let mut workers = resolve_upload_workers(&args.common, &satellite).await?;
    if args.replication_factor == 0 {
        return Err(anyhow::anyhow!(
            "--replication-factor must be >= 1 for upload"
        ));
    }
    if args.replication_factor > workers.len() {
        return Err(anyhow::anyhow!(
            "replication factor {} exceeds connected workers {}",
            args.replication_factor,
            workers.len()
        ));
    }

    let input_bytes = fs::read(&args.input)?;
    let file_id = args.file_id.unwrap_or_else(create_file_id);
    let original_hash_hex = hash::blake3_hash_hex(&input_bytes);

    let mut segments = Vec::new();
    let mut rr = 0usize;

    for (segment_index, chunk) in input_bytes.chunks(SEGMENT_SIZE).enumerate() {
        let segment_index = segment_index as u32;
        let key = crypto::derive_file_key(ROOT_SECRET, &format!("{file_id}:{segment_index}"));
        let (ciphertext, nonce) = crypto::encrypt_aes256gcm(&key, chunk)?;
        let ciphertext_len = ciphertext.len();

        let shards = sharding::rs_encode(&ciphertext)?;
        for (shard_index, shard_bytes) in shards.into_iter().enumerate() {
            let shard_index = shard_index as u8;
            let base_idx = rr % workers.len();
            rr += 1;

            let shard_hash_hex = hash::blake3_hash_hex(&shard_bytes);
            for replica_offset in 0..args.replication_factor {
                let idx = (base_idx + replica_offset) % workers.len();
                let worker = &mut workers[idx];
                match worker
                    .client
                    .request(NetRequest::StoreShard {
                        file_id: file_id.clone(),
                        segment_index,
                        shard_index,
                        bytes: shard_bytes.clone(),
                    })
                    .await?
                {
                    NetResponse::StoreShardAck { stored: true, .. } => {}
                    NetResponse::StoreShardAck { stored: false, .. } => {
                        return Err(anyhow::anyhow!(
                            "worker {} reported stored=false for shard {shard_index} replica {}",
                            worker.worker_id,
                            replica_offset
                        ));
                    }
                    NetResponse::Error { message } => {
                        return Err(anyhow::anyhow!(
                            "store shard error on worker {} replica {}: {message}",
                            worker.worker_id,
                            replica_offset
                        ));
                    }
                    other => {
                        return Err(anyhow::anyhow!("unexpected store response: {other:?}"));
                    }
                }

                satellite
                    .register_shard(ShardRecord {
                        worker_id: worker.worker_id.clone(),
                        worker_multiaddr: worker.multiaddr.to_string(),
                        file_id: file_id.clone(),
                        segment_index,
                        shard_index,
                        shard_hash_hex: shard_hash_hex.clone(),
                    })
                    .await?;
            }
        }

        segments.push(ManifestSegment {
            segment_index,
            plaintext_len: chunk.len() as u64,
            ciphertext_len: ciphertext_len as u64,
            nonce,
        });
    }

    let manifest = FileManifest {
        file_id: file_id.clone(),
        original_len: input_bytes.len() as u64,
        original_hash_hex,
        segments,
    };
    let uploader_kp = identity::load_or_create_keypair_for("uplink")?;
    let signed_manifest = SignedManifest::sign(manifest, &uploader_kp)?;
    let manifest_path = save_manifest(&signed_manifest)?;
    satellite.register_manifest(&signed_manifest).await?;

    println!("Upload complete");
    println!("file_id={file_id}");
    println!("workers_connected={}", workers.len());
    println!("replication_factor={}", args.replication_factor);
    println!("manifest={}", manifest_path.display());
    println!("segments={}", signed_manifest.manifest.segments.len());
    println!("manifest_registered=true");

    Ok(())
}

async fn run_download(args: DownloadArgs) -> Result<()> {
    let satellite = SatelliteClient::new(args.common.satellite_url.clone());
    let signed_manifest = match load_manifest(&args.file_id) {
        Ok(local) => local,
        Err(_) => {
            let remote = satellite.manifest(&args.file_id).await?;
            let _ = save_manifest(&remote);
            remote
        }
    };
    signed_manifest.verify()?;
    let manifest = &signed_manifest.manifest;
    let locate = satellite.locate(&args.file_id).await?;

    if locate.shards.is_empty() {
        return Err(anyhow::anyhow!("satellite returned no shards for file"));
    }

    let mut worker_addr_by_id: HashMap<String, Multiaddr> = HashMap::new();
    for rec in &locate.shards {
        if let Ok(addr) = rec.worker_multiaddr.parse() {
            worker_addr_by_id
                .entry(rec.worker_id.clone())
                .or_insert(addr);
        }
    }

    let mut workers: HashMap<String, WorkerConnection> = HashMap::new();
    for (worker_id, addr) in worker_addr_by_id {
        match connect_worker(&args.common, addr.clone()).await {
            Ok(conn) => {
                workers.insert(worker_id, conn);
            }
            Err(err) => {
                eprintln!("warning: worker {worker_id} offline/unreachable ({addr}): {err}");
            }
        }
    }

    let mut shard_map_by_segment: HashMap<u32, Vec<ShardRecord>> = HashMap::new();
    for rec in locate.shards {
        shard_map_by_segment
            .entry(rec.segment_index)
            .or_default()
            .push(rec);
    }

    let mut restored = Vec::new();

    for segment in &manifest.segments {
        let seg_records = shard_map_by_segment
            .get(&segment.segment_index)
            .ok_or_else(|| anyhow::anyhow!("no records for segment {}", segment.segment_index))?;

        let mut available_workers: HashSet<String> = HashSet::new();
        for rec in seg_records {
            if workers.contains_key(&rec.worker_id) {
                available_workers.insert(rec.worker_id.clone());
            }
        }
        if available_workers.is_empty() {
            return Err(anyhow::anyhow!(
                "no online workers for segment {}",
                segment.segment_index
            ));
        }

        let mut per_worker_prepare: HashMap<String, Vec<u8>> = HashMap::new();
        for rec in seg_records {
            if workers.contains_key(&rec.worker_id) {
                per_worker_prepare
                    .entry(rec.worker_id.clone())
                    .or_default()
                    .push(rec.shard_index);
            }
        }

        for (worker_id, indices) in per_worker_prepare {
            if let Some(conn) = workers.get_mut(&worker_id) {
                let mut uniq = indices;
                uniq.sort_unstable();
                uniq.dedup();
                match conn
                    .client
                    .request(NetRequest::Prepare {
                        file_id: args.file_id.clone(),
                        segment_index: segment.segment_index,
                        shard_indices: uniq,
                    })
                    .await
                {
                    Ok(NetResponse::PrepareAck { .. }) => {}
                    Ok(other) => {
                        eprintln!("warning: prepare unexpected for worker {worker_id}: {other:?}");
                        conn.online = false;
                    }
                    Err(err) => {
                        eprintln!("warning: prepare failed for worker {worker_id}: {err}");
                        conn.online = false;
                    }
                }
            }
        }

        let mut shard_options: Vec<Option<Vec<u8>>> = vec![None; RS_N];
        let mut used_shards = 0usize;

        let mut unique_by_index: BTreeMap<u8, Vec<&ShardRecord>> = BTreeMap::new();
        for rec in seg_records {
            unique_by_index
                .entry(rec.shard_index)
                .or_default()
                .push(rec);
        }

        for (shard_index, candidates) in unique_by_index {
            if used_shards >= RS_K {
                break;
            }

            let mut success = false;
            for rec in candidates {
                let Some(conn) = workers.get_mut(&rec.worker_id) else {
                    continue;
                };
                if !conn.online {
                    continue;
                }

                match conn
                    .client
                    .request(NetRequest::VerifyGet {
                        file_id: args.file_id.clone(),
                        segment_index: segment.segment_index,
                        shard_index,
                    })
                    .await
                {
                    Ok(NetResponse::VerifyGetOk { bytes, .. }) => {
                        let actual_hash = hash::blake3_hash_hex(&bytes);
                        if actual_hash != rec.shard_hash_hex {
                            eprintln!(
                                "warning: hash mismatch segment {} shard {} from worker {}",
                                segment.segment_index, shard_index, rec.worker_id
                            );
                            continue;
                        }

                        shard_options[shard_index as usize] = Some(bytes);
                        used_shards += 1;
                        success = true;
                        break;
                    }
                    Ok(NetResponse::Error { message }) => {
                        eprintln!(
                            "warning: verify_get error segment {} shard {} from worker {}: {}",
                            segment.segment_index, shard_index, rec.worker_id, message
                        );
                    }
                    Ok(other) => {
                        eprintln!(
                            "warning: unexpected verify_get response from worker {}: {other:?}",
                            rec.worker_id
                        );
                    }
                    Err(err) => {
                        eprintln!(
                            "warning: worker {} went offline while fetching shard {}: {}",
                            rec.worker_id, shard_index, err
                        );
                        conn.online = false;
                    }
                }
            }

            if !success {
                continue;
            }
        }

        if used_shards < RS_K {
            return Err(anyhow::anyhow!(
                "segment {} only recovered {} shards, need {}",
                segment.segment_index,
                used_shards,
                RS_K
            ));
        }

        let encrypted = sharding::rs_reconstruct(shard_options, segment.ciphertext_len as usize)?;
        let key = crypto::derive_file_key(
            ROOT_SECRET,
            &format!("{}:{}", manifest.file_id, segment.segment_index),
        );
        let mut plaintext = crypto::decrypt_aes256gcm(&key, &encrypted, &segment.nonce)?;
        plaintext.truncate(segment.plaintext_len as usize);
        restored.extend_from_slice(&plaintext);
    }

    fs::write(&args.output, &restored)?;

    let restored_hash_hex = hash::blake3_hash_hex(&restored);
    let equal =
        restored_hash_hex == manifest.original_hash_hex && restored.len() as u64 == manifest.original_len;

    println!("Download complete");
    println!("file_id={}", manifest.file_id);
    println!(
        "workers_online={}",
        workers.values().filter(|w| w.online).count()
    );
    println!("original_hash={}", manifest.original_hash_hex);
    println!("restored_hash={restored_hash_hex}");
    println!("equal={equal}");
    println!("output={}", args.output.display());

    if !equal {
        return Err(anyhow::anyhow!("restored data does not match original"));
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    match parse_command()? {
        Command::Upload(args) => run_upload(args).await,
        Command::Download(args) => run_download(args).await,
        Command::Repair(args) => run_repair(args).await,
    }
}
