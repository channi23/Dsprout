use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetRequest {
    Hello {
        from_peer: String,
        message: String,
    },
    Prepare {
        file_id: String,
        segment_index: u32,
        shard_indices: Vec<u8>,
    },
    VerifyGet {
        file_id: String,
        segment_index: u32,
        shard_index: u8,
    },
    StoreShard {
        file_id: String,
        segment_index: u32,
        shard_index: u8,
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetResponse {
    HelloAck {
        from_peer: String,
        message: String,
    },
    PrepareAck {
        file_id: String,
        segment_index: u32,
        loaded: usize,
        missing: Vec<u8>,
    },
    VerifyGetOk {
        file_id: String,
        segment_index: u32,
        shard_index: u8,
        bytes: Vec<u8>,
        blake3_hash: [u8; 32],
        source: String,
    },
    StoreShardAck {
        file_id: String,
        segment_index: u32,
        shard_index: u8,
        stored: bool,
    },
    Error {
        message: String,
    },
}
