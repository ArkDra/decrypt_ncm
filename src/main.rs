mod decrypt;
mod file;
mod keybox;
mod metadata;

use crate::decrypt::*;
use crate::file::*;
use crate::metadata::*;

use anyhow::{Context, Result, bail};
use rayon::iter::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use std::fs;
use std::io::{BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

const CORE_KEY: [u8; 16] = [
    0x68, 0x7a, 0x48, 0x52, 0x41, 0x6d, 0x73, 0x6f, 0x35, 0x6b, 0x49, 0x6e, 0x62, 0x61, 0x78, 0x57,
];
const META_KEY: [u8; 16] = [
    0x23, 0x31, 0x34, 0x6C, 0x6A, 0x6B, 0x5F, 0x21, 0x5C, 0x5D, 0x26, 0x30, 0x55, 0x3C, 0x27, 0x28,
];

fn output_filename(title: &str, artists: &str, extension: &str) -> String {
    let name = if artists.trim().is_empty() {
        title.to_string()
    } else {
        format!("{title} - {artists}")
    };
    let mut name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let device_name = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(device_name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            device_name.strip_prefix(prefix).is_some_and(|suffix| {
                (suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
                    || matches!(suffix, "¹" | "²" | "³")
            })
        })
    {
        name.insert(0, '_');
    }
    // Leave room for the extension and stay below common 255-byte component limits.
    let mut end = name.len().min(240 - extension.len() - 1);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name.truncate(end);
    let name = name.trim_end_matches([' ', '.']);
    let name = if name.is_empty() { "Untitled" } else { name };
    format!("{name}.{extension}")
}

fn process_input(path: &Path, overwrite: bool, jobs: usize) -> Result<()> {
    let output_writer = OutputWriter::new(overwrite);

    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }

    if path.is_file() {
        if is_ncm_file(path) {
            decrypt_file(path, &output_writer)
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

        if ncm_files.is_empty() {
            return Ok(());
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs.min(ncm_files.len()))
            .build()
            .context("failed to create file conversion thread pool")?;
        let errors: Vec<String> = pool.install(|| {
            ncm_files
                .par_iter()
                .filter_map(|file_path| {
                    decrypt_file(file_path, &output_writer)
                        .with_context(|| format!("failed to decrypt {}", file_path.display()))
                        .err()
                        .map(|err| format!("{}: {:#}", file_path.display(), err))
                })
                .collect()
        });

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

fn decrypt_file(file_path: &Path, output_writer: &OutputWriter) -> Result<()> {
    let reader = open_file(file_path)?;

    let (mut audio, cover_data, mut meta_data) =
        process_encrypted_data(reader).with_context(|| {
            format!(
                "failed to process encrypted data for {}",
                file_path.display()
            )
        })?;

    normalize_metadata(&mut audio, &mut meta_data, file_path)?;
    audio
        .rewind()
        .context("failed to rewind audio after format detection")?;
    let music_name = meta_str(&meta_data, "musicName")?;
    let ext = meta_str(&meta_data, "format")?;
    let artists = process_artist(&meta_data);
    let file_name = output_filename(music_name, &artists, ext);
    let output_path = file_path.with_file_name(file_name);

    output_writer.write(
        file_path,
        &output_path,
        |temporary_file| {
            let mut writer = BufWriter::with_capacity(BUFFER_SIZE, temporary_file);
            std::io::copy(&mut audio, &mut writer).context("failed to decrypt and copy audio")?;
            writer.flush().context("failed to flush audio output")?;
            Ok(())
        },
        |temporary_path| {
            add_meta_info(temporary_path, &meta_data, cover_data)
                .with_context(|| format!("failed to write tags for {}", output_path.display()))
        },
    )?;

    Ok(())
}

fn run() -> Result<()> {
    let usage = "usage: decrypt_ncm [--overwrite] [--jobs <count>] <file_or_directory>";
    let mut overwrite = false;
    let mut jobs = 2;
    let mut input_path = None;
    let mut positional_only = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if !positional_only && arg == "--overwrite" {
            overwrite = true;
        } else if !positional_only && arg == "--jobs" {
            let count = args.next().context("--jobs requires a positive integer")?;
            jobs = count
                .to_str()
                .context("--jobs requires a positive integer")?
                .parse::<usize>()
                .context("--jobs requires a positive integer")?;
            if jobs == 0 {
                bail!("--jobs requires a positive integer");
            }
        } else if !positional_only && arg == "--" {
            positional_only = true;
        } else if !positional_only && arg.to_string_lossy().starts_with('-') {
            bail!("unknown option: {}\n{}", arg.to_string_lossy(), usage);
        } else if input_path.replace(PathBuf::from(arg)).is_some() {
            bail!("expected one input path\n{}", usage);
        }
    }
    let input_path = input_path.context(usage)?;
    process_input(&input_path, overwrite, jobs)
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{:#}", err);
        std::process::exit(1);
    }
}
