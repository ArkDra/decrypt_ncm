use super::process_encrypted_data;
use base64::{Engine, prelude::BASE64_STANDARD};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

#[path = "../../tests/common/mod.rs"]
mod common;

const SONG: &[u8] = include_bytes!("../../tests/fixtures/song.ncm");
const CIPHER_CYCLE: &[u8] = include_bytes!("../../tests/fixtures/stream-cycle.bin");

fn assert_parse_error(data: &[u8], expected: &str) {
    let error = process_encrypted_data(Cursor::new(data))
        .err()
        .expect("malformed input must be rejected");
    let message = format!("{error:#}");
    assert!(message.contains(expected), "unexpected error: {message}");
}

#[test]
fn lengths_are_bounded_by_limits_and_remaining_data() {
    let (metadata_length, cover_space, _) = common::sections(SONG);
    for (offset, length, field) in [
        (10, 65_537_u32, "core key"),
        (metadata_length, 1_048_577, "metadata"),
        (cover_space, 16_777_217, "cover space"),
    ] {
        let mut data = SONG.to_vec();
        data[offset..offset + 4].copy_from_slice(&length.to_le_bytes());
        let error = process_encrypted_data(Cursor::new(data))
            .err()
            .expect("oversized length must be rejected");
        let message = format!("{error:#}");
        assert!(message.contains(field) && message.contains("exceeds limit"));
    }

    assert_parse_error(&SONG[..metadata_length + 5], "truncated metadata data");

    let mut data = SONG.to_vec();
    // There is already a nonempty cover; declaring zero space must be rejected.
    data[cover_space..cover_space + 4].copy_from_slice(&0_u32.to_le_bytes());
    assert_parse_error(&data, "exceeds cover space");
}

#[test]
fn invalid_envelope_prefixes_are_rejected() {
    let (metadata_length, cover_space, _) = common::sections(SONG);
    let metadata_start = metadata_length + 4;
    let metadata_end = cover_space - 5;
    let mut bad_key = SONG.to_vec();
    // ECB corruption in the first block leaves the final padding block intact.
    bad_key[14] ^= 1;
    assert_parse_error(&bad_key, "invalid core key prefix");

    let mut bad_outer_prefix = SONG.to_vec();
    bad_outer_prefix[metadata_start] ^= 1;
    assert_parse_error(&bad_outer_prefix, "invalid metadata base64 prefix");

    let outer: Vec<u8> = SONG[metadata_start..metadata_end]
        .iter()
        .map(|byte| byte ^ 0x63)
        .collect();
    let mut ciphertext = BASE64_STANDARD.decode(&outer[22..]).unwrap();
    ciphertext[0] ^= 1;
    let mut replacement = outer[..22].to_vec();
    replacement.extend(BASE64_STANDARD.encode(ciphertext).bytes());
    let mut bad_inner_prefix = SONG.to_vec();
    for (target, byte) in bad_inner_prefix[metadata_start..metadata_end]
        .iter_mut()
        .zip(replacement)
    {
        *target = byte ^ 0x63;
    }
    assert_parse_error(&bad_inner_prefix, "invalid metadata JSON prefix");
}

struct ShortReads<R>(R);

impl<R: Read> Read for ShortReads<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let limit = buffer.len().min(7);
        self.0.read(&mut buffer[..limit])
    }
}

impl<R: Seek> Seek for ShortReads<R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.0.seek(from)
    }
}

#[test]
fn stream_reads_preserve_offsets_across_chunks_short_reads_and_seeks() {
    let (_, _, audio_start) = common::sections(SONG);
    // The fixed vector decrypts to 0..=255; no cipher is implemented in the test.
    let expected: Vec<u8> = (0..65_555).map(|index| index as u8).collect();
    let mut data = SONG[..audio_start].to_vec();
    data.extend(CIPHER_CYCLE.iter().copied().cycle().take(expected.len()));

    for chunk_size in [255, 257, 16_385] {
        let (mut audio, _, _) = process_encrypted_data(Cursor::new(&data)).unwrap();
        let mut buffer = vec![0; chunk_size];
        let mut decoded = Vec::new();
        loop {
            let count = audio.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            decoded.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(decoded, expected, "chunk size {chunk_size}");
    }

    let (mut audio, _, _) = process_encrypted_data(ShortReads(Cursor::new(&data))).unwrap();
    let mut decoded = Vec::new();
    io::copy(&mut audio, &mut decoded).unwrap();
    assert_eq!(decoded, expected);

    audio.seek(SeekFrom::Start(255)).unwrap();
    let mut middle = [0; 257];
    audio.read_exact(&mut middle).unwrap();
    assert_eq!(&middle, &expected[255..512]);
    audio.seek(SeekFrom::Current(-4)).unwrap();
    let mut last = [0; 4];
    audio.read_exact(&mut last).unwrap();
    assert_eq!(&last, &expected[508..512]);
    audio.seek(SeekFrom::End(-9)).unwrap();
    let mut tail = [0; 9];
    audio.read_exact(&mut tail).unwrap();
    assert_eq!(&tail, &expected[expected.len() - 9..]);
    audio.rewind().unwrap();
    audio.read_exact(&mut last).unwrap();
    assert_eq!(&last, &expected[..4]);

    let (mut audio, _, _) = process_encrypted_data(Cursor::new(data)).unwrap();
    audio.reader.get_mut().truncate(audio_start + 1);
    let error = io::copy(&mut audio, &mut io::sink()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}
