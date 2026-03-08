use anyhow::Result;
use libp2p::identity;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct StoredKeypair {
    // Protobuf-encoded libp2p keypair bytes.
    keypair_bytes: Vec<u8>,
}

fn identity_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("No data_dir found"))?;
    let dir = base.join("dsprout");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn keypair_path() -> Result<PathBuf> {
    Ok(identity_dir()?.join("identity_ed25519.json"))
}

fn keypair_path_for(profile: &str) -> Result<PathBuf> {
    Ok(identity_dir()?.join(format!("identity_ed25519_{profile}.json")))
}

fn load_or_create_keypair_at(path: PathBuf) -> Result<identity::Keypair> {
    let path = path;

    if path.exists() {
        let bytes = fs::read(&path)?;
        let stored: StoredKeypair = serde_json::from_slice(&bytes)?;
        let kp = identity::Keypair::from_protobuf_encoding(&stored.keypair_bytes)?;
        return Ok(kp);
    }

    let kp = identity::Keypair::generate_ed25519();
    let keypair_bytes = kp.to_protobuf_encoding()?;
    let stored = StoredKeypair { keypair_bytes };

    fs::write(&path, serde_json::to_vec_pretty(&stored)?)?;
    Ok(kp)
}

/// Loads existing default keypair if present; otherwise generates and persists.
pub fn load_or_create_keypair() -> Result<identity::Keypair> {
    load_or_create_keypair_at(keypair_path()?)
}

/// Loads existing profile keypair if present; otherwise generates and persists.
pub fn load_or_create_keypair_for(profile: &str) -> Result<identity::Keypair> {
    load_or_create_keypair_at(keypair_path_for(profile)?)
}

pub fn peer_id_from_keypair(kp: &identity::Keypair) -> libp2p::PeerId {
    libp2p::PeerId::from(kp.public())
}
