use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

pub fn is_ncm_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("ncm"))
        .unwrap_or(false)
}

pub fn read_file(file_path: &Path) -> Result<Vec<u8>> {
    let file = File::open(file_path)
        .with_context(|| format!("failed to open file: {}", file_path.display()))?;
    let mut buf = BufReader::new(file);
    let mut data = Vec::new();
    buf.read_to_end(&mut data)
        .with_context(|| format!("failed to read file: {}", file_path.display()))?;
    Ok(data)
}

pub fn write_file(file_path: &Path, data: &[u8]) -> Result<()> {
    let mut file = File::create(file_path)
        .with_context(|| format!("failed to create file: {}", file_path.display()))?;
    file.write_all(data)
        .with_context(|| format!("failed to write file: {}", file_path.display()))?;
    Ok(())
}
