use anyhow::{Context, Result, bail};
use lofty::{
    config::WriteOptions,
    file::FileType,
    picture::{Picture, PictureType},
    probe::Probe,
    tag::{Accessor, Tag, TagExt},
};
use serde_json::Value;
use std::io::{BufReader, Cursor};
use std::path::Path;

pub fn meta_str<'a>(meta_data: &'a Value, key: &str) -> Result<&'a str> {
    meta_data
        .get(key)
        .and_then(|value| value.as_str())
        .with_context(|| format!("missing or invalid '{}' in metadata", key))
}

fn audio_format(meta_data: &Value) -> Result<FileType> {
    match meta_str(meta_data, "format")? {
        "flac" => Ok(FileType::Flac),
        "mp3" => Ok(FileType::Mpeg),
        other => bail!("unsupported audio format: {}", other),
    }
}

pub fn normalize_metadata(
    music_data: &[u8],
    meta_data: &mut Value,
    input_path: &Path,
) -> Result<()> {
    // Reject an explicitly unsupported declaration before inspecting the audio.
    let expected = match meta_data.get("format") {
        None | Some(Value::Null) => None,
        Some(Value::String(format)) if format.is_empty() => None,
        Some(_) => Some(audio_format(meta_data)?),
    };
    let actual = Probe::new(Cursor::new(music_data))
        .guess_file_type()
        .context("failed to detect audio format")?
        .file_type()
        .context("unrecognized audio format")?;
    let format = match actual {
        FileType::Flac => "flac",
        FileType::Mpeg => "mp3",
        other => bail!("unsupported detected audio format: {:?}", other),
    };
    if let Some(expected) = expected
        && actual != expected
    {
        bail!(
            "audio format does not match metadata: expected {:?}, detected {:?}",
            expected,
            actual
        );
    }
    meta_data["format"] = Value::String(format.to_string());
    if optional_meta_str(meta_data, "musicName").is_none() {
        let title = input_path
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Untitled".to_string());
        meta_data["musicName"] = Value::String(title);
    }
    Ok(())
}

fn optional_meta_str<'a>(meta_data: &'a Value, key: &str) -> Option<&'a str> {
    meta_data
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub fn process_artist(meta_data: &Value) -> String {
    let mut artists = String::new();
    if let Some(artist_array) = meta_data.get("artist").and_then(|a| a.as_array()) {
        if artist_array.len() > 1 {
            let mut artist_names = Vec::new();
            for item in artist_array {
                if let Some(name) = item.get(0).and_then(|n| n.as_str()) {
                    artist_names.push(name.to_string());
                }
            }
            artists = artist_names.join(",");
        } else if let Some(name) = artist_array
            .first()
            .and_then(|a| a.get(0))
            .and_then(|n| n.as_str())
        {
            artists = name.to_string();
        }
    }
    artists
}

pub fn add_meta_info(output_path: &Path, meta_data: &Value, cover_data: Vec<u8>) -> Result<()> {
    let tag_type = audio_format(meta_data)?.primary_tag_type();
    let mut tag = Tag::new(tag_type);

    let music_name = meta_str(meta_data, "musicName")?;
    let artist = process_artist(meta_data);

    if !cover_data.is_empty() {
        let mut cover_buf = BufReader::new(cover_data.as_slice());
        let mut cover =
            Picture::from_reader(&mut cover_buf).context("failed to decode cover image")?;
        cover.set_pic_type(PictureType::CoverFront);
        tag.push_picture(cover);
    }

    tag.set_title(music_name.to_string());
    if !artist.trim().is_empty() {
        tag.set_artist(artist);
    }
    if let Some(album_name) = optional_meta_str(meta_data, "album") {
        tag.set_album(album_name.to_string());
    }

    tag.save_to_path(output_path, WriteOptions::default())
        .with_context(|| format!("failed to write tags to {}", output_path.display()))?;

    Ok(())
}
