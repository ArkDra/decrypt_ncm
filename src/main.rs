mod cli;
mod decrypt;
mod file;
mod keybox;
mod metadata;

use crate::decrypt::*;
use crate::file::*;
use crate::metadata::*;

use anyhow::{Context, Result, bail};
use clap::Parser;
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

fn process_input(args: &cli::Args) -> Result<()> {
    let path = &args.input;
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect input {}", path.display()))?;
    let mut ncm_files = Vec::new();
    let mut skipped = 0;
    let mut errors = Vec::new();

    if metadata.is_file() {
        if !is_ncm_file(path) {
            bail!("file is not .ncm: {}", path.display());
        }
        ncm_files.push(path.clone());
    } else if metadata.is_dir() {
        let entries = fs::read_dir(path)
            .with_context(|| format!("failed to read directory {}", path.display()))?;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    errors.push(format!(
                        "failed to read directory entry in {}: {}",
                        path.display(),
                        err
                    ));
                    continue;
                }
            };
            let file_path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_file() && is_ncm_file(&file_path) => {
                    ncm_files.push(file_path);
                }
                Ok(_) => skipped += 1,
                Err(err) => errors.push(format!(
                    "failed to inspect directory entry {}: {}",
                    file_path.display(),
                    err
                )),
            }
        }
    } else {
        bail!("path is not a file or directory: {}", path.display());
    }

    let mut succeeded = 0;
    if ncm_files.is_empty() {
        println!("No regular .ncm files found in {}.", path.display());
    } else {
        ncm_files.sort();
        let output_dir = prepare_output_dir(args.output_dir.as_deref())?;
        let output_writer = OutputWriter::new(args.overwrite);
        let convert = |file_path: &PathBuf| {
            decrypt_file(file_path, output_dir.as_deref(), &output_writer)
                .with_context(|| format!("failed to decrypt {}", file_path.display()))
                .err()
                .map(|err| format!("{}: {:#}", file_path.display(), err))
        };
        let conversion_errors: Vec<String> = if ncm_files.len() == 1 {
            ncm_files.iter().filter_map(convert).collect()
        } else {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(args.jobs.get().min(ncm_files.len()))
                .build()
                .context("failed to create file conversion thread pool")?;
            pool.install(|| ncm_files.par_iter().filter_map(convert).collect())
        };
        succeeded = ncm_files.len() - conversion_errors.len();
        errors.extend(conversion_errors);
    }

    println!(
        "Summary: {} succeeded, {} failed, {} skipped",
        succeeded,
        errors.len(),
        skipped
    );
    if !errors.is_empty() {
        bail!(
            "failed to decrypt {} file(s):\n{}",
            errors.len(),
            errors.join("\n")
        );
    }

    Ok(())
}

fn prepare_output_dir(directory: Option<&Path>) -> Result<Option<PathBuf>> {
    directory
        .map(|directory| {
            fs::create_dir_all(directory).with_context(|| {
                format!("failed to create output directory {}", directory.display())
            })?;
            fs::canonicalize(directory).with_context(|| {
                format!("failed to resolve output directory {}", directory.display())
            })
        })
        .transpose()
}

fn decrypt_file(
    file_path: &Path,
    output_dir: Option<&Path>,
    output_writer: &OutputWriter,
) -> Result<()> {
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
    let output_path = match output_dir {
        Some(directory) => directory.join(file_name),
        None => file_path.with_file_name(file_name),
    };

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
    process_input(&cli::Args::parse())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{:#}", err);
        std::process::exit(1);
    }
}
