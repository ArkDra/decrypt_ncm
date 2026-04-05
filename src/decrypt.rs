use std::io::{Cursor, Read, Seek};

use anyhow::{bail, Context, Result};
use aes::{
    Aes128,
    cipher::{BlockDecryptMut, KeyInit, block_padding::Pkcs7, generic_array::GenericArray},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde_json::Value;

use crate::keybox::KeyBox;
use crate::{CORE_KEY, META_KEY};

pub fn aes_decrypt(mut data: Vec<u8>, key: &[u8]) -> Result<Vec<u8>> {
    let key = GenericArray::from_slice(key);
    let cipher = ecb::Decryptor::<Aes128>::new(key);
    match cipher.decrypt_padded_mut::<Pkcs7>(&mut data) {
        Ok(data) => Ok(data.to_vec()),
        Err(e) => Err(anyhow::anyhow!("aes decrypt failed (invalid padding or key): {:?}", e)),
    }
}

pub fn process_encrypted_data(data: Vec<u8>) -> Result<(Vec<u8>, Vec<u8>, Value)> {
    let mut cursor = Cursor::new(data);

    let mut header = [0u8; 8];
    cursor
        .read_exact(&mut header)
        .context("failed to read ncm header")?;

    if header != [0x43, 0x54, 0x45, 0x4E, 0x46, 0x44, 0x41, 0x4D] {
        bail!("invalid ncm header (expected CTENFDAM)");
    }

    cursor
        .seek(std::io::SeekFrom::Current(2))
        .context("failed to seek to key length")?;

    let mut key_length = [0u8; 4];
    cursor
        .read_exact(&mut key_length)
        .context("failed to read key length")?;
    let key_length = usize::try_from(u32::from_le_bytes(key_length))
        .context("invalid key length")?;

    let mut key_data = vec![0u8; key_length];
    cursor
        .read_exact(&mut key_data)
        .context("failed to read key data")?;

    for byte in &mut key_data {
        *byte ^= 0x64;
    }

    let key_data = aes_decrypt(key_data, &CORE_KEY)
        .context("failed to decrypt core key data")?;
    if key_data.len() <= 17 {
        bail!("decrypted key data too short: {}", key_data.len());
    }
    let key_box = KeyBox::new(&key_data[17..])
        .context("failed to build key box")?;

    let mut meta_length = [0u8; 4];
    cursor
        .read_exact(&mut meta_length)
        .context("failed to read meta length")?;
    let meta_length = usize::try_from(u32::from_le_bytes(meta_length))
        .context("invalid meta length")?;

    let mut meta_data = vec![0u8; meta_length];
    cursor
        .read_exact(&mut meta_data)
        .context("failed to read meta data")?;

    for byte in &mut meta_data {
        *byte ^= 0x63;
    }

    if meta_data.len() < 22 {
        bail!("meta data too short for base64 payload: {}", meta_data.len());
    }
    let meta_data = BASE64_STANDARD
        .decode(&meta_data[22..])
        .context("failed to base64 decode meta data")?;
    let meta_data = aes_decrypt(meta_data, &META_KEY)
        .context("failed to decrypt meta data")?;
    if meta_data.len() < 6 {
        bail!("meta data too short for json payload: {}", meta_data.len());
    }
    let meta_data: Value = serde_json::from_slice(&meta_data[6..])
        .context("failed to parse meta json")?;

    cursor
        .seek(std::io::SeekFrom::Current(9))
        .context("failed to seek to cover length")?;

    let mut cover_length = [0u8; 4];
    cursor
        .read_exact(&mut cover_length)
        .context("failed to read cover length")?;
    let cover_length = usize::try_from(u32::from_le_bytes(cover_length))
        .context("invalid cover length")?;

    let mut cover_data = vec![0u8; cover_length];
    cursor
        .read_exact(&mut cover_data)
        .context("failed to read cover data")?;

    let mut music_data = Vec::new();
    cursor
        .read_to_end(&mut music_data)
        .context("failed to read music data")?;
    let music_data = key_box.apply_keystream(music_data);

    Ok((music_data, cover_data, meta_data))
}