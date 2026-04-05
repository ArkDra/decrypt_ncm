mod decrypt;
mod file;
mod keybox;
mod metadata;

use crate::decrypt::*;
use crate::file::*;
use crate::metadata::*;

use anyhow::{bail, Context, Result};
use rayon::iter::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use std::fs;
use std::path::Path;

const CORE_KEY: [u8; 16] = [
    0x68, 0x7a, 0x48, 0x52, 0x41, 0x6d, 0x73, 0x6f, 0x35, 0x6b, 0x49, 0x6e, 0x62, 0x61, 0x78, 0x57,
];
const META_KEY: [u8; 16] = [
    0x23, 0x31, 0x34, 0x6C, 0x6A, 0x6B, 0x5F, 0x21, 0x5C, 0x5D, 0x26, 0x30, 0x55, 0x3C, 0x27, 0x28,
];

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn process_input(input_path: &str) -> Result<()> {
    let path = Path::new(input_path);

    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }

    if path.is_file() {
        if is_ncm_file(path) {
            decrypt_file(path)
                .with_context(|| format!("failed to decrypt {}", path.display()))?;
        } else {
            bail!("file is not .ncm: {}", path.display());
        }
    } else if path.is_dir() {
        let entries = fs::read_dir(path)
            .with_context(|| format!("failed to read directory {}", path.display()))?;
        let mut ncm_files = Vec::new();

        for entry in entries {
            let entry = entry
                .with_context(|| format!("failed to read directory entry in {}", path.display()))?;
            let file_path = entry.path();
            if is_ncm_file(&file_path) {
                ncm_files.push(file_path);
            }
        }

        let errors: Vec<String> = ncm_files
            .par_iter()
            .filter_map(|file_path| {
                decrypt_file(file_path)
                    .with_context(|| format!("failed to decrypt {}", file_path.display()))
                    .err()
                    .map(|err| format!("{}: {:#}", file_path.display(), err))
            })
            .collect();

        if !errors.is_empty() {
            bail!(
                "failed to decrypt {} file(s):\n{}",
                errors.len(),
                errors.join("\n")
            );
        }
    } else {
        bail!("path is not a file or directory: {}", path.display());
    }

    Ok(())
}

fn decrypt_file(file_path: &Path) -> Result<()> {
    let file_data = read_file(file_path)
        .with_context(|| format!("failed to read {}", file_path.display()))?;

    let (music_data, cover_data, meta_data) = process_encrypted_data(file_data)
        .with_context(|| format!("failed to process encrypted data for {}", file_path.display()))?;

    let music_name = meta_str(&meta_data, "musicName")?;
    let ext = meta_str(&meta_data, "format")?;
    let artists = process_artist(&meta_data);
    let file_name = sanitize_filename(&format!("{} - {}.{}", music_name, artists, ext));
    let output_path = file_path.with_file_name(file_name);

    write_file(&output_path, &music_data)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    add_meta_info(&output_path, &meta_data, cover_data)
        .with_context(|| format!("failed to write tags for {}", output_path.display()))?;

    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        bail!("usage: decrypt_ncm <file_or_directory>");
    }

    let input_path = &args[1];
    process_input(input_path)
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{:#}", err);
        std::process::exit(1);
    }
}
