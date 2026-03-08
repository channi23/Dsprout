use serde::{Deserialize, Serialize};

pub const SEGMENT_SIZE: usize = 64 * 1024 * 1024; // 64MB
pub const RS_K: usize = 29;
pub const RS_N: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMeta {
    pub file_id: String,
    pub segment_index: u32,
    pub plaintext_len: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMeta {
    pub file_id: String,
    pub segment_index: u32,
    pub shard_index: u8, // 0..79
    pub blake3_hash: [u8; 32],
    pub nonce: [u8; 12], // encryption nonce (if per-segment)
}
