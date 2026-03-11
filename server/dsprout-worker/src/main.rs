mod store;

use anyhow::Result;
use dsprout_common::{
    hash,
    models::{RegisterShardReq, RegisterWorkerReq, ShardRecord},
    net::{
        DsproutEvent, build_swarm,
        hello::{NetRequest, NetResponse},
    },
};
use futures::StreamExt;
use libp2p::{Multiaddr, request_response, swarm::SwarmEvent};
use serde::Serialize;
use std::env;

#[derive(Debug, Clone)]
struct RunArgs {
    profile: String,
    listen: Multiaddr,
    advertise_multiaddr: Option<Multiaddr>,
    satellite_url: String,
    device_name: String,
    owner_label: String,
    capacity_limit_bytes: u64,
    enabled: bool,
}

#[derive(Debug, Serialize, Clone)]
struct WorkerHeartbeat {
    worker_id: String,
    multiaddr: String,
    device_name: Option<String>,
    owner_label: Option<String>,
    capacity_limit_bytes: Option<u64>,
    used_bytes: Option<u64>,
    enabled: Option<bool>,
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err(anyhow::anyhow!("hex data must have even length"));
    }

    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let pair = std::str::from_utf8(&bytes[i..i + 2])?;
        let b = u8::from_str_radix(pair, 16)?;
        out.push(b);
    }
    Ok(out)
}

fn parse_seed_args(args: &[String]) -> Result<(String, u32, u8, Vec<u8>)> {
    let mut file_id: Option<String> = None;
    let mut segment: Option<u32> = None;
    let mut shard: Option<u8> = None;
    let mut data: Option<Vec<u8>> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--file-id" => {
                i += 1;
                file_id = Some(
                    args.get(i)
                        .ok_or_else(|| anyhow::anyhow!("missing --file-id value"))?
                        .clone(),
                );
            }
            "--segment" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing --segment value"))?;
                segment = Some(v.parse()?);
            }
            "--shard" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing --shard value"))?;
                shard = Some(v.parse()?);
            }
            "--data" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing --data value"))?;
                data = Some(v.as_bytes().to_vec());
            }
            "--data-hex" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing --data-hex value"))?;
                data = Some(decode_hex(v)?);
            }
            unknown => {
                return Err(anyhow::anyhow!("unknown seed argument: {unknown}"));
            }
        }
        i += 1;
    }

    let file_id = file_id.ok_or_else(|| anyhow::anyhow!("--file-id is required"))?;
    let segment = segment.ok_or_else(|| anyhow::anyhow!("--segment is required"))?;
    let shard = shard.ok_or_else(|| anyhow::anyhow!("--shard is required"))?;
    let data = data.ok_or_else(|| anyhow::anyhow!("--data or --data-hex is required"))?;

    Ok((file_id, segment, shard, data))
}

fn parse_run_args(args: &[String]) -> Result<RunArgs> {
    let mut profile = "worker".to_string();
    let mut listen: Multiaddr = "/ip4/0.0.0.0/tcp/4001".parse()?;
    let mut advertise_multiaddr: Option<Multiaddr> = None;
    let mut satellite_url = "http://127.0.0.1:7070".to_string();
    let mut device_name: Option<String> = None;
    let mut owner_label = "local-contributor".to_string();
    let mut capacity_limit_bytes: u64 = 10 * 1024 * 1024 * 1024;
    let mut enabled = true;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => {
                i += 1;
                profile = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --profile"))?
                    .clone();
            }
            "--listen" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --listen"))?;
                listen = v.parse()?;
            }
            "--advertise-multiaddr" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --advertise-multiaddr"))?;
                advertise_multiaddr = Some(v.parse()?);
            }
            "--satellite-url" => {
                i += 1;
                satellite_url = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --satellite-url"))?
                    .clone();
            }
            "--device-name" => {
                i += 1;
                device_name = Some(
                    args.get(i)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --device-name"))?
                        .clone(),
                );
            }
            "--owner-label" => {
                i += 1;
                owner_label = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --owner-label"))?
                    .clone();
            }
            "--capacity-limit-bytes" => {
                i += 1;
                capacity_limit_bytes = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --capacity-limit-bytes"))?
                    .parse()?;
            }
            "--enabled" => {
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --enabled"))?;
                enabled = match v.as_str() {
                    "1" | "true" | "TRUE" | "True" => true,
                    "0" | "false" | "FALSE" | "False" => false,
                    _ => return Err(anyhow::anyhow!("--enabled must be true/false/1/0")),
                };
            }
            unknown => return Err(anyhow::anyhow!("unknown argument: {unknown}")),
        }
        i += 1;
    }

    Ok(RunArgs {
        device_name: device_name.unwrap_or_else(|| profile.clone()),
        owner_label,
        capacity_limit_bytes,
        enabled,
        profile,
        listen,
        advertise_multiaddr,
        satellite_url,
    })
}

async fn post_json(url: &str, path: &str, payload: &impl Serialize) {
    let endpoint = format!(
        "{}/{}",
        url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let client = reqwest::Client::new();
    if let Err(err) = client.post(endpoint).json(payload).send().await {
        eprintln!("satellite call failed: {err}");
    }
}

async fn post_json_result(url: &str, path: &str, payload: &impl Serialize) -> Result<()> {
    let endpoint = format!(
        "{}/{}",
        url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let client = reqwest::Client::new();
    let res = client.post(endpoint).json(payload).send().await?;
    res.error_for_status()?;
    Ok(())
}

async fn reregister_local_shards(
    satellite_url: &str,
    worker_id: &str,
    worker_multiaddr: &str,
) -> Result<usize> {
    let discovered = store::scan_local_shards()?;
    let mut registered = 0usize;

    for shard in discovered {
        let req = RegisterShardReq {
            record: ShardRecord {
                worker_id: worker_id.to_string(),
                worker_multiaddr: worker_multiaddr.to_string(),
                file_id: shard.file_id,
                segment_index: shard.segment_index,
                shard_index: shard.shard_index,
                shard_hash_hex: hash::blake3_hash_hex(&shard.bytes),
            },
        };

        match post_json_result(satellite_url, "/register_shard", &req).await {
            Ok(()) => {
                registered += 1;
            }
            Err(err) => {
                eprintln!("warning: failed to re-register shard at startup: {err}");
            }
        }
    }

    Ok(registered)
}

fn used_bytes_from_local_scan() -> u64 {
    match store::scan_local_shards() {
        Ok(shards) => shards.iter().map(|s| s.bytes.len() as u64).sum(),
        Err(_) => 0,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();

    // Temporary local seeding command for testing VerifyGet before upload flow exists.
    if args.first().map(String::as_str) == Some("seed") {
        let (file_id, segment, shard, bytes) = parse_seed_args(&args[1..])?;
        store::save_shard(&file_id, segment, shard, &bytes).await?;
        println!(
            "Seeded shard: file_id={file_id} segment={segment} shard={shard} bytes={} blake3={}",
            bytes.len(),
            hash::blake3_hash_hex(&bytes)
        );
        return Ok(());
    }

    let run = parse_run_args(&args)?;
    store::set_store_profile(run.profile.clone());
    let hot_cache = store::HotCache::new();

    let swarm_key = dsprout_common::net::default_swarm_key_path(env!("CARGO_MANIFEST_DIR"));
    let mut swarm = build_swarm(&swarm_key, &run.profile)?;

    swarm.listen_on(run.listen.clone())?;

    let worker_id = swarm.local_peer_id().to_string();
    let advertised_multiaddr = run
        .advertise_multiaddr
        .clone()
        .unwrap_or_else(|| run.listen.clone());
    println!("Worker peer id: {worker_id}");
    println!("Worker profile: {}", run.profile);
    println!("Worker listening on: {}", run.listen);
    println!("Worker advertised as: {}", advertised_multiaddr);

    let reg = RegisterWorkerReq {
        worker_id: worker_id.clone(),
        multiaddr: advertised_multiaddr.to_string(),
        device_name: run.device_name.clone(),
        owner_label: run.owner_label.clone(),
        capacity_limit_bytes: run.capacity_limit_bytes,
        used_bytes: used_bytes_from_local_scan(),
        enabled: run.enabled,
    };
    post_json(&run.satellite_url, "/register_worker", &reg).await;
    match reregister_local_shards(&run.satellite_url, &reg.worker_id, &reg.multiaddr).await {
        Ok(count) => {
            println!("Startup shard inventory re-registered: {count}");
        }
        Err(err) => {
            eprintln!("warning: startup shard inventory scan failed: {err}");
        }
    }

    let satellite_url = run.satellite_url.clone();
    let heartbeat_base = WorkerHeartbeat {
        worker_id: reg.worker_id.clone(),
        multiaddr: reg.multiaddr.clone(),
        device_name: Some(reg.device_name.clone()),
        owner_label: Some(reg.owner_label.clone()),
        capacity_limit_bytes: Some(reg.capacity_limit_bytes),
        used_bytes: None,
        enabled: Some(reg.enabled),
    };
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            ticker.tick().await;
            let mut payload = heartbeat_base.clone();
            payload.used_bytes = Some(used_bytes_from_local_scan());
            post_json(&satellite_url, "/heartbeat", &payload).await;
        }
    });

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {address}");
            }
            SwarmEvent::Behaviour(DsproutEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message:
                        request_response::Message::Request {
                            request, channel, ..
                        },
                },
            )) => {
                let response = match request {
                    NetRequest::Hello { from_peer, message } => {
                        println!("Received hello from {peer} ({from_peer}): {message}");
                        NetResponse::HelloAck {
                            from_peer: swarm.local_peer_id().to_string(),
                            message: "hello-ack-from-worker".to_string(),
                        }
                    }
                    NetRequest::Prepare {
                        file_id,
                        segment_index,
                        shard_indices,
                    } => {
                        let summary = store::preload_shards(
                            &hot_cache,
                            &file_id,
                            segment_index,
                            &shard_indices,
                        )
                        .await?;

                        NetResponse::PrepareAck {
                            file_id,
                            segment_index,
                            loaded: summary.loaded,
                            missing: summary.missing,
                        }
                    }
                    NetRequest::VerifyGet {
                        file_id,
                        segment_index,
                        shard_index,
                    } => match store::get_shard_prefer_cache(
                        &hot_cache,
                        &file_id,
                        segment_index,
                        shard_index,
                    )
                    .await
                    {
                        Ok((bytes, source)) => NetResponse::VerifyGetOk {
                            file_id,
                            segment_index,
                            shard_index,
                            blake3_hash: hash::blake3_hash(&bytes),
                            bytes,
                            source: match source {
                                store::ShardSource::Ram => "ram".to_string(),
                                store::ShardSource::Disk => "disk".to_string(),
                            },
                        },
                        Err(err) => NetResponse::Error {
                            message: format!("verify_get failed: {err}"),
                        },
                    },
                    NetRequest::StoreShard {
                        file_id,
                        segment_index,
                        shard_index,
                        bytes,
                    } => {
                        match store::save_shard(&file_id, segment_index, shard_index, &bytes).await
                        {
                            Ok(()) => NetResponse::StoreShardAck {
                                file_id,
                                segment_index,
                                shard_index,
                                stored: true,
                            },
                            Err(err) => NetResponse::Error {
                                message: format!("store_shard failed: {err}"),
                            },
                        }
                    }
                };

                if let Err(err) = swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
                {
                    eprintln!("Failed to send response: {err:?}");
                }
            }
            SwarmEvent::Behaviour(DsproutEvent::Identify(event)) => {
                println!("Identify event: {event:?}");
            }
            SwarmEvent::Behaviour(DsproutEvent::RequestResponse(event)) => {
                println!("Request-response event: {event:?}");
            }
            SwarmEvent::IncomingConnectionError { error, .. } => {
                eprintln!("Incoming connection error: {error}");
            }
            SwarmEvent::OutgoingConnectionError { error, .. } => {
                eprintln!("Outgoing connection error: {error}");
            }
            _ => {}
        }
    }
}
