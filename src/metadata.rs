use anyhow::{bail, Context, Result};
use lofty::{
    config::WriteOptions,
    picture::{Picture, PictureType},
    tag::{Accessor, Tag, TagExt, TagType},
};
use serde_json::Value;
use std::io::BufReader;
use std::path::Path;

pub fn meta_str<'a>(meta_data: &'a Value, key: &str) -> Result<&'a str> {
    meta_data
        .get(key)
        .and_then(|value| value.as_str())
        .with_context(|| format!("missing or invalid '{}' in metadata", key))
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
    let format = meta_str(meta_data, "format")?;
    let tag_type = match format {
        "flac" => TagType::VorbisComments,
        "mp3" => TagType::Id3v2,
        other => bail!("unsupported audio format for tags: {}", other),
    };
    let mut tag = Tag::new(tag_type);

    let music_name = meta_str(meta_data, "musicName")?;
    let album_name = meta_str(meta_data, "album")?;
    let artist = process_artist(meta_data);

    let mut cover_buf = BufReader::new(cover_data.as_slice());
    let mut cover = Picture::from_reader(&mut cover_buf)
        .context("failed to decode cover image")?;
    cover.set_pic_type(PictureType::CoverFront);

    tag.push_picture(cover);
    tag.set_title(music_name.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album_name.to_string());

    tag.save_to_path(output_path, WriteOptions::default())
        .with_context(|| format!("failed to write tags to {}", output_path.display()))?;

    Ok(())
}
