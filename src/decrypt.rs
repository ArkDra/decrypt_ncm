use std::io::{Cursor, Read, Seek, SeekFrom};

use aes::{
    Aes128,
    cipher::{BlockDecryptMut, KeyInit, block_padding::Pkcs7},
};
use anyhow::{Context, Result, bail};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde_json::Value;

use crate::keybox::KeyBox;
use crate::{CORE_KEY, META_KEY};

const MAX_KEY_LENGTH: usize = 64 * 1024;
const MAX_META_LENGTH: usize = 1024 * 1024;
const MAX_COVER_LENGTH: usize = 16 * 1024 * 1024;
const KEY_PREFIX: &[u8] = b"neteasecloudmusic";
const META_PREFIX: &[u8] = b"163 key(Don't modify):";
const JSON_PREFIX: &[u8] = b"music:";

fn read_length(cursor: &mut Cursor<Vec<u8>>, field: &str) -> Result<usize> {
    let mut length = [0u8; 4];
    cursor
        .read_exact(&mut length)
        .with_context(|| format!("failed to read {field} length"))?;
    usize::try_from(u32::from_le_bytes(length)).with_context(|| format!("invalid {field} length"))
}

fn check_length(cursor: &Cursor<Vec<u8>>, length: usize, limit: usize, field: &str) -> Result<()> {
    if length > limit {
        bail!("{field} length {length} exceeds limit {limit}");
    }
    let remaining = (cursor.get_ref().len() as u64)
        .checked_sub(cursor.position())
        .context("parser position exceeds file length")?;
    if length as u64 > remaining {
        bail!("truncated {field} data: declared {length} bytes, remaining {remaining}");
    }
    Ok(())
}

fn read_payload(
    cursor: &mut Cursor<Vec<u8>>,
    length: usize,
    limit: usize,
    field: &str,
) -> Result<Vec<u8>> {
    check_length(cursor, length, limit, field)?;
    let mut data = vec![0; length];
    cursor
        .read_exact(&mut data)
        .with_context(|| format!("failed to read {field} data"))?;
    Ok(data)
}

pub fn aes_decrypt(mut data: Vec<u8>, key: &[u8]) -> Result<Vec<u8>> {
    let cipher = ecb::Decryptor::<Aes128>::new_from_slice(key)
        .map_err(|_| anyhow::anyhow!("invalid AES key length"))?;
    cipher
        .decrypt_padded_mut::<Pkcs7>(&mut data)
        .map(|data| data.to_vec())
        .map_err(|err| anyhow::anyhow!("AES decrypt failed (invalid padding or key): {err:?}"))
}

fn decrypt_metadata(mut data: Vec<u8>) -> Result<Value> {
    if data.is_empty() {
        return Ok(serde_json::json!({}));
    }
    for byte in &mut data {
        *byte ^= 0x63;
    }
    let payload = data
        .strip_prefix(META_PREFIX)
        .context("invalid metadata base64 prefix")?;
    let payload = BASE64_STANDARD
        .decode(payload)
        .context("failed to base64 decode metadata")?;
    let payload = aes_decrypt(payload, &META_KEY).context("failed to decrypt metadata")?;
    let json = payload
        .strip_prefix(JSON_PREFIX)
        .context("invalid metadata JSON prefix")?;
    let metadata: Value = serde_json::from_slice(json).context("failed to parse metadata JSON")?;
    if !metadata.is_object() {
        bail!("metadata JSON must be an object");
    }
    Ok(metadata)
}

pub fn process_encrypted_data(data: Vec<u8>) -> Result<(Vec<u8>, Vec<u8>, Value)> {
    let mut cursor = Cursor::new(data);
    let mut header = [0u8; 8];
    cursor
        .read_exact(&mut header)
        .context("failed to read NCM header")?;
    if &header != b"CTENFDAM" {
        bail!("invalid NCM header (expected CTENFDAM)");
    }
    let mut reserved = [0; 2];
    cursor
        .read_exact(&mut reserved)
        .context("failed to read NCM reserved bytes")?;

    let key_length = read_length(&mut cursor, "core key")?;
    let mut key_data = read_payload(&mut cursor, key_length, MAX_KEY_LENGTH, "core key")?;
    for byte in &mut key_data {
        *byte ^= 0x64;
    }
    let key_data = aes_decrypt(key_data, &CORE_KEY).context("failed to decrypt core key data")?;
    let key = key_data
        .strip_prefix(KEY_PREFIX)
        .context("invalid core key prefix")?;
    let key_box = KeyBox::new(key).context("failed to build key box")?;

    let meta_length = read_length(&mut cursor, "metadata")?;
    let meta_data = read_payload(&mut cursor, meta_length, MAX_META_LENGTH, "metadata")?;
    let meta_data = decrypt_metadata(meta_data)?;

    let mut crc_and_reserved = [0; 5];
    cursor
        .read_exact(&mut crc_and_reserved)
        .context("failed to read CRC and reserved bytes")?;
    let cover_space = read_length(&mut cursor, "cover space")?;
    let cover_length = read_length(&mut cursor, "cover")?;
    check_length(&cursor, cover_space, MAX_COVER_LENGTH, "cover space")?;
    if cover_length > cover_space {
        bail!("cover length {cover_length} exceeds cover space {cover_space}");
    }
    let cover_data = read_payload(&mut cursor, cover_length, MAX_COVER_LENGTH, "cover")?;
    cursor
        .seek(SeekFrom::Current((cover_space - cover_length) as i64))
        .context("failed to skip cover padding")?;

    let mut music_data = Vec::new();
    cursor
        .read_to_end(&mut music_data)
        .context("failed to read audio data")?;
    if music_data.is_empty() {
        bail!("missing audio data");
    }
    let music_data = key_box.apply_keystream(music_data);
    Ok((music_data, cover_data, meta_data))
}
