use lofty::{
    config::WriteOptions,
    picture::{MimeType, Picture, PictureType},
    tag::{Accessor, Tag, TagExt, TagType},
};
use serde_json::Value;

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

pub fn add_meta_info(output_path: &str, meta_data: &Value, cover_data: Vec<u8>) {
    let tag_type = TagType::VorbisComments;
    let mut tag = Tag::new(tag_type);

    let music_name = meta_data["musicName"].as_str().unwrap();
    let album_name = meta_data["album"].as_str().unwrap();
    let artist = process_artist(&meta_data);
    let artist = artist.as_str();

    let cover = Picture::new_unchecked(
        PictureType::CoverFront,
        Some(MimeType::Jpeg),
        None,
        cover_data,
    );
    tag.push_picture(cover);
    tag.set_title(music_name.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album_name.to_string());

    tag.save_to_path(output_path, WriteOptions::default())
        .unwrap();
}
