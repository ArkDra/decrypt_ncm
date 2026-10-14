use lofty::{
    file::{FileType, TaggedFileExt},
    tag::Accessor,
};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const SONG: &[u8] = include_bytes!("fixtures/song.ncm");
const EXISTING: &[u8] = b"existing output must survive";
const OUTPUT_NAME: &str = "Song - Artist.flac";

fn run(input: &Path, overwrite: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_decrypt_ncm"));
    command.env("RAYON_NUM_THREADS", "2");
    if overwrite {
        command.arg("--overwrite");
    }
    command.arg(input).output().unwrap()
}

fn assert_failure(result: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success(), "unexpected success");
    assert!(stderr.contains(message), "unexpected error: {stderr}");
}

fn assert_files(directory: &Path, expected: &[&str]) {
    let mut actual: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    actual.sort();
    let mut expected: Vec<_> = expected.iter().map(|name| name.to_string()).collect();
    expected.sort();
    assert_eq!(actual, expected, "unexpected output or temporary file");
}

fn assert_complete_output(path: &Path, format: FileType) {
    let audio = lofty::read_from_path(path).unwrap();
    assert_eq!(audio.file_type(), format);
    let tag = audio.primary_tag().unwrap();
    assert_eq!(tag.title().as_deref(), Some("Song"));
    assert_eq!(tag.artist().as_deref(), Some("Artist"));
    assert_eq!(tag.album().as_deref(), Some("Album"));
    assert_eq!(tag.pictures().len(), 1);
}

#[test]
fn existing_output_requires_explicit_overwrite() {
    for (fixture, output_name, format) in [
        (SONG, OUTPUT_NAME, FileType::Flac),
        (
            include_bytes!("fixtures/song-mp3.ncm").as_slice(),
            "Song - Artist.mp3",
            FileType::Mpeg,
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("input.ncm");
        let output = directory.path().join(output_name);
        fs::write(&source, fixture).unwrap();
        fs::write(&output, EXISTING).unwrap();

        assert_failure(&run(&source, false), "output already exists");
        assert_eq!(fs::read(&output).unwrap(), EXISTING);
        assert_files(directory.path(), &["input.ncm", output_name]);

        let result = run(&source, true);
        assert!(result.status.success(), "{:?}", result);
        assert_complete_output(&output, format);
        assert_eq!(fs::read(&source).unwrap(), fixture);
        assert_files(directory.path(), &["input.ncm", output_name]);
    }
}

#[test]
fn invalid_formats_never_replace_source_or_output() {
    for (fixture, error) in [
        (
            include_bytes!("fixtures/unsupported-format.ncm").as_slice(),
            "unsupported audio format",
        ),
        (
            include_bytes!("fixtures/mismatched-format.ncm").as_slice(),
            "audio format does not match metadata",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        // An unsupported `ncm` format would previously overwrite this input itself.
        let source = directory.path().join("Song - Artist.ncm");
        let output = directory.path().join("Song - Artist.mp3");
        fs::write(&source, fixture).unwrap();
        fs::write(&output, EXISTING).unwrap();

        assert_failure(&run(&source, true), error);
        assert_eq!(fs::read(&source).unwrap(), fixture);
        assert_eq!(fs::read(&output).unwrap(), EXISTING);
        assert_files(
            directory.path(),
            &["Song - Artist.ncm", "Song - Artist.mp3"],
        );
    }
}

#[test]
fn input_aliases_cannot_be_overwritten() {
    let check_alias = |create_alias: fn(&Path, &Path) -> std::io::Result<()>| {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("input.ncm");
        let output = directory.path().join(OUTPUT_NAME);
        fs::write(&source, SONG).unwrap();
        create_alias(&source, &output).unwrap();

        assert_failure(&run(&source, true), "output refers to the input file");
        assert_eq!(fs::read(&source).unwrap(), SONG);
        assert_eq!(fs::read(&output).unwrap(), SONG);
        assert!(same_file::is_same_file(&source, &output).unwrap());
        assert_files(directory.path(), &["input.ncm", OUTPUT_NAME]);
    };
    check_alias(|source, output| fs::hard_link(source, output));
    #[cfg(unix)]
    check_alias(|source, output| std::os::unix::fs::symlink(source, output));
}

#[test]
fn failed_tags_leave_destination_unchanged() {
    let fixture = include_bytes!("fixtures/bad-cover.ncm");
    for existing_output in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("input.ncm");
        let output = directory.path().join(OUTPUT_NAME);
        fs::write(&source, fixture).unwrap();
        if existing_output {
            fs::write(&output, EXISTING).unwrap();
        }

        assert_failure(&run(&source, true), "failed to decode cover image");
        assert_eq!(fs::read(&source).unwrap(), fixture);
        if existing_output {
            assert_eq!(fs::read(&output).unwrap(), EXISTING);
            assert_files(directory.path(), &["input.ncm", OUTPUT_NAME]);
        } else {
            assert_files(directory.path(), &["input.ncm"]);
        }
    }
}

#[test]
fn batch_duplicate_targets_have_one_winner() {
    for overwrite in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.ncm");
        let second = directory.path().join("second.ncm");
        fs::write(&first, SONG).unwrap();
        fs::write(&second, SONG).unwrap();

        assert_failure(
            &run(directory.path(), overwrite),
            "failed to decrypt 1 file(s)",
        );
        assert_complete_output(&directory.path().join(OUTPUT_NAME), FileType::Flac);
        assert_eq!(fs::read(&first).unwrap(), SONG);
        assert_eq!(fs::read(&second).unwrap(), SONG);
        assert_files(directory.path(), &["first.ncm", "second.ncm", OUTPUT_NAME]);
    }
}
