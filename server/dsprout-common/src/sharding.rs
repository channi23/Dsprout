use anyhow::{Result, anyhow};
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::models::{RS_K, RS_N};

pub fn rs_encode(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let shard_len = data.len().div_ceil(RS_K);
    let padded_len = shard_len * RS_K;

    let mut padded = vec![0u8; padded_len];
    padded[..data.len()].copy_from_slice(data);

    let mut shards: Vec<Vec<u8>> = (0..RS_N).map(|_| vec![0u8; shard_len]).collect();

    for (i, shard) in shards.iter_mut().take(RS_K).enumerate() {
        let start = i * shard_len;
        let end = start + shard_len;
        shard.copy_from_slice(&padded[start..end]);
    }

    let r = ReedSolomon::new(RS_K, RS_N - RS_K)?;
    r.encode(&mut shards)?;

    Ok(shards)
}

pub fn rs_reconstruct(mut shards: Vec<Option<Vec<u8>>>, original_len: usize) -> Result<Vec<u8>> {
    if shards.len() != RS_N {
        return Err(anyhow!("expected {RS_N} shards"));
    }

    let shard_len = shards
        .iter()
        .flatten()
        .next()
        .ok_or_else(|| anyhow!("no shards"))?
        .len();

    let r = ReedSolomon::new(RS_K, RS_N - RS_K)?;
    r.reconstruct(&mut shards)?;

    let mut data = Vec::with_capacity(shard_len * RS_K);
    for (i, shard_opt) in shards.iter().enumerate().take(RS_K) {
        let shard = shard_opt
            .as_ref()
            .ok_or_else(|| anyhow!("missing data shard {i}"))?;
        data.extend_from_slice(shard);
    }

    data.truncate(original_len);
    Ok(data)
}
