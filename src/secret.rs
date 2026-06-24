use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::{env, fs};

const SEALED_PREFIX: &str = "pv1:";

pub fn seal(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    let key = machine_key();
    let mut out = plain.as_bytes().to_vec();
    xor_keystream(&mut out, &key);
    format!("{SEALED_PREFIX}{}", STANDARD.encode(out))
}

pub fn open(sealed: &str) -> String {
    if sealed.is_empty() {
        return String::new();
    }
    let payload = sealed.strip_prefix(SEALED_PREFIX).unwrap_or(sealed);
    let Ok(mut bytes) = STANDARD.decode(payload) else {
        return String::new();
    };
    let key = machine_key();
    xor_keystream(&mut bytes, &key);
    String::from_utf8(bytes).unwrap_or_default()
}

pub fn has_secret(sealed: &str) -> bool {
    !sealed.trim().is_empty()
}

fn machine_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"popovic-v0-local-secret");
    if let Ok(value) = env::var("POPOVIC_MASTER_KEY") {
        hasher.update(value.as_bytes());
    }
    if let Ok(machine_id) = fs::read("/etc/machine-id") {
        hasher.update(&machine_id);
    }
    if let Ok(home) = env::var("HOME") {
        hasher.update(home.as_bytes());
    }
    hasher.finalize().into()
}

fn xor_keystream(bytes: &mut [u8], key: &[u8; 32]) {
    for (index, byte) in bytes.iter_mut().enumerate() {
        let lane = key[index % key.len()];
        let mixed = lane.rotate_left((index % 8) as u32) ^ ((index * 31) as u8);
        *byte ^= mixed;
    }
}
