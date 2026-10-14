use std::io::{self, Read, Seek, SeekFrom};

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

pub struct NcmReader<R> {
    reader: R,
    key_box: KeyBox,
    audio_start: u64,
    audio_length: u64,
    position: u64,
}

impl<R: Read> Read for NcmReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.audio_length - self.position;
        let limit = remaining.min(buffer.len() as u64) as usize;
        if limit == 0 {
            return Ok(0);
        }
        let read = self.reader.read(&mut buffer[..limit])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "audio data was truncated while reading",
            ));
        }
        self.key_box
            .apply_keystream(&mut buffer[..read], self.position);
        self.position += read as u64;
        Ok(read)
    }
}

impl<R: Seek> Seek for NcmReader<R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.audio_length.checked_add_signed(offset),
        }
        .filter(|&position| position <= self.audio_length)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek outside audio data"))?;
        self.reader
            .seek(SeekFrom::Start(self.audio_start + position))?;
        self.position = position;
        Ok(position)
    }
}

fn read_length<R: Read>(reader: &mut R, field: &str) -> Result<usize> {
    let mut length = [0u8; 4];
    reader
        .read_exact(&mut length)
        .with_context(|| format!("failed to read {field} length"))?;
    usize::try_from(u32::from_le_bytes(length)).with_context(|| format!("invalid {field} length"))
}

fn check_length<R: Seek>(
    reader: &mut R,
    file_length: u64,
    length: usize,
    limit: usize,
    field: &str,
) -> Result<()> {
    if length > limit {
        bail!("{field} length {length} exceeds limit {limit}");
    }
    let remaining = file_length
        .checked_sub(reader.stream_position()?)
        .context("parser position exceeds file length")?;
    if length as u64 > remaining {
        bail!("truncated {field} data: declared {length} bytes, remaining {remaining}");
    }
    Ok(())
}

fn read_payload<R: Read + Seek>(
    reader: &mut R,
    file_length: u64,
    length: usize,
    limit: usize,
    field: &str,
) -> Result<Vec<u8>> {
    check_length(reader, file_length, length, limit, field)?;
    let mut data = vec![0; length];
    reader
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

pub fn process_encrypted_data<R: Read + Seek>(
    mut reader: R,
) -> Result<(NcmReader<R>, Vec<u8>, Value)> {
    let file_length = reader
        .seek(SeekFrom::End(0))
        .context("failed to determine NCM file length")?;
    reader.rewind().context("failed to rewind NCM file")?;
    let mut header = [0u8; 8];
    reader
        .read_exact(&mut header)
        .context("failed to read NCM header")?;
    if &header != b"CTENFDAM" {
        bail!("invalid NCM header (expected CTENFDAM)");
    }
    let mut reserved = [0; 2];
    reader
        .read_exact(&mut reserved)
        .context("failed to read NCM reserved bytes")?;

    let key_length = read_length(&mut reader, "core key")?;
    let mut key_data = read_payload(
        &mut reader,
        file_length,
        key_length,
        MAX_KEY_LENGTH,
        "core key",
    )?;
    for byte in &mut key_data {
        *byte ^= 0x64;
    }
    let key_data = aes_decrypt(key_data, &CORE_KEY).context("failed to decrypt core key data")?;
    let key = key_data
        .strip_prefix(KEY_PREFIX)
        .context("invalid core key prefix")?;
    let key_box = KeyBox::new(key).context("failed to build key box")?;

    let meta_length = read_length(&mut reader, "metadata")?;
    let meta_data = read_payload(
        &mut reader,
        file_length,
        meta_length,
        MAX_META_LENGTH,
        "metadata",
    )?;
    let meta_data = decrypt_metadata(meta_data)?;

    let mut crc_and_reserved = [0; 5];
    reader
        .read_exact(&mut crc_and_reserved)
        .context("failed to read CRC and reserved bytes")?;
    let cover_space = read_length(&mut reader, "cover space")?;
    let cover_length = read_length(&mut reader, "cover")?;
    check_length(
        &mut reader,
        file_length,
        cover_space,
        MAX_COVER_LENGTH,
        "cover space",
    )?;
    if cover_length > cover_space {
        bail!("cover length {cover_length} exceeds cover space {cover_space}");
    }
    let cover_data = read_payload(
        &mut reader,
        file_length,
        cover_length,
        MAX_COVER_LENGTH,
        "cover",
    )?;
    reader
        .seek(SeekFrom::Current((cover_space - cover_length) as i64))
        .context("failed to skip cover padding")?;

    let audio_start = reader
        .stream_position()
        .context("failed to locate audio data")?;
    let audio_length = file_length
        .checked_sub(audio_start)
        .context("audio start exceeds NCM file length")?;
    if audio_length == 0 {
        bail!("missing audio data");
    }
    let audio = NcmReader {
        reader,
        key_box,
        audio_start,
        audio_length,
        position: 0,
    };
    Ok((audio, cover_data, meta_data))
}
