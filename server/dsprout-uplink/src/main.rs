use anyhow::Result;
use dsprout_common::{
    crypto, hash,
    models::{RS_K, RS_N, SEGMENT_SIZE},
    net::{
        DsproutBehaviour, DsproutEvent, build_swarm,
        hello::{NetRequest, NetResponse},
    },
    sharding,
};
use futures::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm, request_response, swarm::SwarmEvent};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    path::PathBuf,
    time::Duration,
};

const ROOT_SECRET: &[u8] = b"dsprout-dev-root-secret-v1";

#[derive(Debug, Clone)]
struct CommonArgs {
    dial: Multiaddr,
    satellite_url: String,
    swarm_key: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct UploadArgs {
    common: CommonArgs,
    input: PathBuf,
    file_id: Option<String>,
}

#[derive(Debug, Clone)]
struct DownloadArgs {
    common: CommonArgs,
    file_id: String,
    output: PathBuf,
}

#[derive(Debug, Clone)]
enum Command {
    Upload(UploadArgs),
    Download(DownloadArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SegmentManifest {
    segment_index: u32,
    plaintext_len: usize,
    ciphertext_len: usize,
    nonce: [u8; 12],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadManifest {
    file_id: String,
    original_len: usize,
    original_hash_hex: String,
    segments: Vec<SegmentManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShardRecord {
    worker_id: String,
    file_id: String,
    segment_index: u32,
    shard_index: u8,
    shard_hash_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterShardReq {
    record: ShardRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocateResp {
    file_id: String,
    shards: Vec<ShardRecord>,
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

fn parse_command() -> Result<Command> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err(anyhow::anyhow!(
            "usage: dsprout-uplink <upload|download> [args]"
        ));
    }

    let subcommand = args.remove(0);
    let mut dial: Option<Multiaddr> = None;
    let mut satellite_url: String = "http://127.0.0.1:7070".to_string();
    let mut swarm_key: Option<PathBuf> = None;

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut file_id: Option<String> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--dial" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --dial"))?;
                dial = Some(v.parse()?);
            }
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
            unknown => {
                return Err(anyhow::anyhow!("unknown argument: {unknown}"));
            }
        }
        i += 1;
    }

    let dial = dial.ok_or_else(|| anyhow::anyhow!("--dial is required"))?;
    let common = CommonArgs {
        dial,
        satellite_url,
        swarm_key,
    };

    match subcommand.as_str() {
        "upload" => Ok(Command::Upload(UploadArgs {
            common,
            input: input.ok_or_else(|| anyhow::anyhow!("--input is required for upload"))?,
            file_id,
        })),
        "download" => Ok(Command::Download(DownloadArgs {
            common,
            file_id: file_id
                .ok_or_else(|| anyhow::anyhow!("--file-id is required for download"))?,
            output: output.ok_or_else(|| anyhow::anyhow!("--output is required for download"))?,
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

fn save_manifest(manifest: &UploadManifest) -> Result<PathBuf> {
    let path = manifest_path(&manifest.file_id)?;
    let bytes = serde_json::to_vec_pretty(manifest)?;
    fs::write(&path, bytes)?;
    Ok(path)
}

fn load_manifest(file_id: &str) -> Result<UploadManifest> {
    let bytes = fs::read(manifest_path(file_id)?)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn create_file_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn run_upload(args: UploadArgs) -> Result<()> {
    let swarm_key = args
        .common
        .swarm_key
        .unwrap_or_else(|| dsprout_common::net::default_swarm_key_path(env!("CARGO_MANIFEST_DIR")));

    let mut client = ControlClient::new(build_swarm(&swarm_key, "uplink")?);
    let worker_peer = client.connect(args.common.dial).await?;

    let satellite = SatelliteClient::new(args.common.satellite_url);

    let input_bytes = fs::read(&args.input)?;
    let file_id = args.file_id.unwrap_or_else(create_file_id);
    let original_hash_hex = hash::blake3_hash_hex(&input_bytes);

    if let NetResponse::Error { message } = client
        .request(NetRequest::Hello {
            from_peer: client.swarm.local_peer_id().to_string(),
            message: "hello-upload".to_string(),
        })
        .await?
    {
        return Err(anyhow::anyhow!("hello failed: {message}"));
    }

    let mut segments = Vec::new();

    for (segment_index, chunk) in input_bytes.chunks(SEGMENT_SIZE).enumerate() {
        let segment_index = segment_index as u32;
        let key = crypto::derive_file_key(ROOT_SECRET, &format!("{file_id}:{segment_index}"));
        let (ciphertext, nonce) = crypto::encrypt_aes256gcm(&key, chunk)?;
        let ciphertext_len = ciphertext.len();

        let shards = sharding::rs_encode(&ciphertext)?;
        for (shard_index, shard_bytes) in shards.into_iter().enumerate() {
            let shard_index = shard_index as u8;
            match client
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
                        "worker reported stored=false for shard {shard_index}"
                    ));
                }
                NetResponse::Error { message } => {
                    return Err(anyhow::anyhow!("store shard error: {message}"));
                }
                other => {
                    return Err(anyhow::anyhow!(
                        "unexpected store shard response: {other:?}"
                    ));
                }
            }

            let shard_hash_hex = hash::blake3_hash_hex(&shard_bytes);
            satellite
                .register_shard(ShardRecord {
                    worker_id: worker_peer.to_string(),
                    file_id: file_id.clone(),
                    segment_index,
                    shard_index,
                    shard_hash_hex,
                })
                .await?;
        }

        segments.push(SegmentManifest {
            segment_index,
            plaintext_len: chunk.len(),
            ciphertext_len,
            nonce,
        });
    }

    let manifest = UploadManifest {
        file_id: file_id.clone(),
        original_len: input_bytes.len(),
        original_hash_hex,
        segments,
    };
    let manifest_path = save_manifest(&manifest)?;

    println!("Upload complete");
    println!("file_id={file_id}");
    println!("worker_peer={worker_peer}");
    println!("manifest={}", manifest_path.display());
    println!("segments={}", manifest.segments.len());

    Ok(())
}

async fn run_download(args: DownloadArgs) -> Result<()> {
    let swarm_key = args
        .common
        .swarm_key
        .unwrap_or_else(|| dsprout_common::net::default_swarm_key_path(env!("CARGO_MANIFEST_DIR")));

    let mut client = ControlClient::new(build_swarm(&swarm_key, "uplink")?);
    let worker_peer = client.connect(args.common.dial).await?;

    let manifest = load_manifest(&args.file_id)?;
    let satellite = SatelliteClient::new(args.common.satellite_url);
    let locate = satellite.locate(&args.file_id).await?;

    if let NetResponse::Error { message } = client
        .request(NetRequest::Hello {
            from_peer: client.swarm.local_peer_id().to_string(),
            message: "hello-download".to_string(),
        })
        .await?
    {
        return Err(anyhow::anyhow!("hello failed: {message}"));
    }

    let mut shard_map_by_segment: HashMap<u32, Vec<ShardRecord>> = HashMap::new();
    for record in locate.shards {
        shard_map_by_segment
            .entry(record.segment_index)
            .or_default()
            .push(record);
    }

    let mut restored = Vec::new();

    for segment in &manifest.segments {
        let seg_records = shard_map_by_segment
            .get(&segment.segment_index)
            .ok_or_else(|| {
                anyhow::anyhow!("no shard records for segment {}", segment.segment_index)
            })?;

        let mut unique: BTreeMap<u8, &ShardRecord> = BTreeMap::new();
        for rec in seg_records {
            unique.entry(rec.shard_index).or_insert(rec);
        }

        if unique.len() < RS_K {
            return Err(anyhow::anyhow!(
                "segment {} has only {} shard records, need at least {}",
                segment.segment_index,
                unique.len(),
                RS_K
            ));
        }

        let selected_indices: Vec<u8> = unique.keys().copied().take(RS_K).collect();

        match client
            .request(NetRequest::Prepare {
                file_id: args.file_id.clone(),
                segment_index: segment.segment_index,
                shard_indices: selected_indices.clone(),
            })
            .await?
        {
            NetResponse::PrepareAck { .. } => {}
            NetResponse::Error { message } => {
                return Err(anyhow::anyhow!("prepare failed: {message}"));
            }
            other => return Err(anyhow::anyhow!("unexpected prepare response: {other:?}")),
        }

        let mut shard_options: Vec<Option<Vec<u8>>> = vec![None; RS_N];
        for shard_index in selected_indices {
            match client
                .request(NetRequest::VerifyGet {
                    file_id: args.file_id.clone(),
                    segment_index: segment.segment_index,
                    shard_index,
                })
                .await?
            {
                NetResponse::VerifyGetOk {
                    shard_index: got_index,
                    bytes,
                    source: _,
                    ..
                } => {
                    if got_index != shard_index {
                        return Err(anyhow::anyhow!(
                            "verify_get mismatch: requested {shard_index}, got {got_index}"
                        ));
                    }

                    let expected = unique.get(&shard_index).ok_or_else(|| {
                        anyhow::anyhow!("missing expected record for shard {shard_index}")
                    })?;
                    let actual_hash = hash::blake3_hash_hex(&bytes);
                    if actual_hash != expected.shard_hash_hex {
                        return Err(anyhow::anyhow!(
                            "hash mismatch for segment {} shard {}",
                            segment.segment_index,
                            shard_index
                        ));
                    }

                    shard_options[shard_index as usize] = Some(bytes);
                }
                NetResponse::Error { message } => {
                    return Err(anyhow::anyhow!("verify_get failed: {message}"));
                }
                other => return Err(anyhow::anyhow!("unexpected verify_get response: {other:?}")),
            }
        }

        let encrypted = sharding::rs_reconstruct(shard_options, segment.ciphertext_len)?;
        let key = crypto::derive_file_key(
            ROOT_SECRET,
            &format!("{}:{}", manifest.file_id, segment.segment_index),
        );
        let mut plaintext = crypto::decrypt_aes256gcm(&key, &encrypted, &segment.nonce)?;
        plaintext.truncate(segment.plaintext_len);
        restored.extend_from_slice(&plaintext);
    }

    fs::write(&args.output, &restored)?;

    let restored_hash_hex = hash::blake3_hash_hex(&restored);
    let equal =
        restored_hash_hex == manifest.original_hash_hex && restored.len() == manifest.original_len;

    println!("Download complete");
    println!("file_id={}", manifest.file_id);
    println!("worker_peer={worker_peer}");
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
    }
}
