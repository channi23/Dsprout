mod store;

use anyhow::Result;
use dsprout_common::{
    hash,
    net::{
        DsproutEvent, build_swarm,
        hello::{NetRequest, NetResponse},
    },
};
use futures::StreamExt;
use libp2p::{Multiaddr, request_response, swarm::SwarmEvent};
use std::env;

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

    let hot_cache = store::HotCache::new();

    let swarm_key = dsprout_common::net::default_swarm_key_path(env!("CARGO_MANIFEST_DIR"));
    let mut swarm = build_swarm(&swarm_key, "worker")?;

    let listen_addr: Multiaddr = "/ip4/0.0.0.0/tcp/4001".parse()?;
    swarm.listen_on(listen_addr.clone())?;

    println!("Worker peer id: {}", swarm.local_peer_id());
    println!("Worker listening on: {listen_addr}");

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
