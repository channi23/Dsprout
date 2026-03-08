use anyhow::Result;
use libp2p::pnet::PreSharedKey;
use std::{fs, path::Path, str::FromStr};

pub fn load_psk(path: impl AsRef<Path>) -> Result<PreSharedKey> {
    let text = fs::read_to_string(path)?;
    let psk = PreSharedKey::from_str(&text)?;
    Ok(psk)
}
