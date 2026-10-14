mod common;

use lofty::{
    file::{FileType, TaggedFileExt},
    tag::Accessor,
};
use std::fs;
use std::process::{Command, Output};

const SONG: &[u8] = include_bytes!("fixtures/song.ncm");
const MP3: &[u8] = include_bytes!("fixtures/song-mp3.ncm");

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_decrypt_ncm"))
}

fn assert_status(result: &Output, code: i32) {
    assert_eq!(
        result.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn assert_summary(result: &Output, succeeded: usize, failed: usize, skipped: usize) {
    let expected = format!("Summary: {succeeded} succeeded, {failed} failed, {skipped} skipped");
    assert!(String::from_utf8_lossy(&result.stdout).contains(&expected));
}

#[test]
fn help_and_version_need_no_input_and_create_no_output_directory() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("must-not-create");
    for (flag, expected) in [
        ("--help", "--output-dir"),
        ("--version", env!("CARGO_PKG_VERSION")),
    ] {
        let result = command()
            .arg("--output-dir")
            .arg(&output)
            .arg(flag)
            .output()
            .unwrap();
        assert_status(&result, 0);
        assert!(String::from_utf8_lossy(&result.stdout).contains(expected));
        assert!(result.stderr.is_empty());
        assert!(!output.exists());
    }
}

#[test]
fn zero_jobs_is_an_argument_error_before_conversion() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("input.ncm");
    let output = directory.path().join("must-not-create");
    fs::write(&source, SONG).unwrap();
    let result = command()
        .args(["--jobs", "0", "--output-dir"])
        .arg(&output)
        .arg(&source)
        .output()
        .unwrap();
    assert_status(&result, 2);
    assert!(String::from_utf8_lossy(&result.stderr).contains("--jobs"));
    assert!(!output.exists());
    assert_eq!(fs::read(source).unwrap(), SONG);
}

#[test]
fn mixed_directory_routes_outputs_and_counts_success_failure_and_skips() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input with spaces");
    let output = directory.path().join("output with spaces/nested");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("good.NCM"), SONG).unwrap();
    fs::write(input.join("good-mp3.ncm"), MP3).unwrap();
    fs::write(input.join("broken.ncm"), b"badheader").unwrap();
    fs::write(input.join("notes.txt"), b"skip").unwrap();
    let nested = input.join("nested.ncm");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("inside.ncm"), SONG).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(input.join("good.NCM"), input.join("alias.ncm")).unwrap();

    let result = command()
        .args(["--jobs", "2", "--output-dir"])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert_status(&result, 1);
    assert_summary(&result, 2, 1, 2 + usize::from(cfg!(unix)));
    assert!(String::from_utf8_lossy(&result.stderr).contains("broken.ncm"));
    for (name, format) in [
        ("Song - Artist.flac", FileType::Flac),
        ("Song - Artist.mp3", FileType::Mpeg),
    ] {
        assert_eq!(
            lofty::read_from_path(output.join(name))
                .unwrap()
                .file_type(),
            format
        );
        assert!(!input.join(name).exists());
        assert!(!nested.join(name).exists());
    }
    assert_eq!(fs::read_dir(&output).unwrap().count(), 2);
    assert_eq!(fs::read(input.join("good.NCM")).unwrap(), SONG);
    assert_eq!(fs::read(input.join("good-mp3.ncm")).unwrap(), MP3);
    assert_eq!(fs::read(nested.join("inside.ncm")).unwrap(), SONG);
}

#[test]
fn empty_directory_reports_no_work() {
    let directory = tempfile::tempdir().unwrap();
    let result = command().arg(directory.path()).output().unwrap();
    assert_status(&result, 0);
    assert_summary(&result, 0, 0, 0);
    assert!(String::from_utf8_lossy(&result.stdout).contains("No regular .ncm files found"));
    assert!(result.stderr.is_empty());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn missing_metadata_uses_filename_and_omits_optional_tags() {
    let (metadata_length, _, audio_start) = common::sections(SONG);
    let mut data = SONG[..metadata_length].to_vec();
    data.extend_from_slice(&[0; 4]); // Zero metadata length.
    data.extend_from_slice(&[0; 13]); // CRC, reserved byte and zero cover lengths.
    data.extend_from_slice(&SONG[audio_start..]);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fallback.ncm");
    fs::write(&source, &data).unwrap();

    let result = command().arg(&source).output().unwrap();
    assert_status(&result, 0);
    assert_summary(&result, 1, 0, 0);
    let audio = lofty::read_from_path(directory.path().join("fallback.flac")).unwrap();
    assert_eq!(audio.file_type(), FileType::Flac);
    let tag = audio.primary_tag().unwrap();
    assert_eq!(tag.title().as_deref(), Some("fallback"));
    assert!(tag.artist().is_none());
    assert!(tag.album().is_none());
    assert!(tag.pictures().is_empty());
    assert_eq!(fs::read(source).unwrap(), data);
}

#[cfg(unix)]
#[test]
fn non_utf8_input_and_output_paths_are_preserved() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir().unwrap();
    let source = directory
        .path()
        .join(OsString::from_vec(b"input-\xff.ncm".to_vec()));
    let output = directory
        .path()
        .join(OsString::from_vec(b"output-\xfe".to_vec()));
    fs::write(&source, SONG).unwrap();
    let result = command()
        .arg("-o")
        .arg(&output)
        .arg(&source)
        .output()
        .unwrap();
    assert_status(&result, 0);
    assert!(output.join("Song - Artist.flac").is_file());
    assert_eq!(fs::read(source).unwrap(), SONG);
}
