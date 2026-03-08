use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Result, anyhow};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;

pub fn derive_file_key(root_secret: &[u8], file_path: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(b"dsprout-salt"), root_secret);
    let mut okm = [0u8; 32];
    hk.expand(file_path.as_bytes(), &mut okm)
        .expect("HKDF expand");
    okm
}

pub fn encrypt_aes256gcm(key32: &[u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new_from_slice(key32)?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow!("AES-256-GCM encryption failed"))?;
    Ok((ciphertext, nonce_bytes))
}

pub fn decrypt_aes256gcm(
    key32: &[u8; 32],
    ciphertext: &[u8],
    nonce12: &[u8; 12],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key32)?;
    let nonce = Nonce::from_slice(nonce12);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow!("AES-256-GCM decryption failed"))?;
    Ok(plaintext)
}
