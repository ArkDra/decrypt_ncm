/// Offsets in a fixed NCM fixture, used only to construct malformed variants.
pub fn sections(data: &[u8]) -> (usize, usize, usize) {
    let length_at =
        |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
    let metadata_length = 14 + length_at(10);
    let cover_space = metadata_length + 4 + length_at(metadata_length) + 5;
    let audio_start = cover_space + 8 + length_at(cover_space);
    (metadata_length, cover_space, audio_start)
}
