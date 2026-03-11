use anyhow::{Result, anyhow};
use libp2p::{PeerId, identity};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSegment {
    pub segment_index: u32,
    pub plaintext_len: u64,
    pub ciphertext_len: u64,
    pub nonce: [u8; 12],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileManifest {
    pub file_id: String,
    pub original_len: u64,
    pub original_hash_hex: String,
    pub segments: Vec<ManifestSegment>,
}

impl FileManifest {
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    pub manifest: FileManifest,
    pub uploader_peer_id: String,
    pub uploader_public_key_protobuf: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedManifest {
    pub fn sign(manifest: FileManifest, uploader_kp: &identity::Keypair) -> Result<Self> {
        let signing_bytes = manifest.signing_bytes()?;
        let signature = uploader_kp
            .sign(&signing_bytes)
            .map_err(|e| anyhow!("failed to sign manifest: {e}"))?;
        let public = uploader_kp.public();
        Ok(Self {
            manifest,
            uploader_peer_id: PeerId::from(public.clone()).to_string(),
            uploader_public_key_protobuf: public.encode_protobuf(),
            signature,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let public = identity::PublicKey::try_decode_protobuf(&self.uploader_public_key_protobuf)
            .map_err(|e| anyhow!("invalid uploader public key: {e}"))?;
        let expected_peer_id = PeerId::from(public.clone()).to_string();
        if self.uploader_peer_id != expected_peer_id {
            return Err(anyhow!(
                "uploader_peer_id does not match uploader public key"
            ));
        }

        let signing_bytes = self.manifest.signing_bytes()?;
        let ok = public.verify(&signing_bytes, &self.signature);
        if !ok {
            return Err(anyhow!("manifest signature verification failed"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub multiaddr: String,
    pub device_name: String,
    pub owner_label: String,
    pub capacity_limit_bytes: u64,
    pub used_bytes: u64,
    pub enabled: bool,
    pub last_seen: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerReq {
    pub worker_id: String,
    pub multiaddr: String,
    pub device_name: String,
    pub owner_label: String,
    pub capacity_limit_bytes: u64,
    pub used_bytes: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkerReq {
    pub worker_id: String,
    pub multiaddr: Option<String>,
    pub device_name: Option<String>,
    pub owner_label: Option<String>,
    pub capacity_limit_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRecord {
    pub worker_id: String,
    pub worker_multiaddr: String,
    pub file_id: String,
    pub segment_index: u32,
    pub shard_index: u8,
    pub shard_hash_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterShardReq {
    pub record: ShardRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocateResp {
    pub file_id: String,
    pub shards: Vec<ShardRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterManifestReq {
    pub signed_manifest: SignedManifest,
}
